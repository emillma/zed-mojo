use zed_extension_api::{
    self as zed, DebugAdapterBinary, DebugConfig, DebugRequest, DebugScenario, DebugTaskDefinition,
    Result, StartDebuggingRequestArguments, StartDebuggingRequestArgumentsRequest, TaskTemplate,
    process::Command as ProcessCommand,
    serde_json::{self, Value},
    settings::LspSettings,
};

const LANGUAGE_SERVER_ID: &str = "mojo-lsp-server";
const DEBUG_ADAPTER_ID: &str = "mojo-lldb";
const LSP_BINARY: &str = "mojo-lsp-server";
const LEGACY_DAP_BINARY: &str = "mojo-lldb-dap";
const DEBUG_LOCATOR_ID: &str = "mojo-source";
const BUILD_SCRIPT: &str = "set -eu\ncd \"$1\"\nshift\nexec \"$@\"";
const CODESIGN_SCRIPT: &str = r#"set -eu
entitlements="$1.entitlements.plist"
printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?>' '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' '<plist version="1.0"><dict><key>com.apple.security.get-task-allow</key><true/></dict></plist>' > "$entitlements"
/usr/bin/codesign -s - -f --entitlements "$entitlements" "$1"
rm -f "$entitlements""#;

const STDLIB_HOME_SCRIPT: &str = "set -eu\ncp \"$2\" \"$1/modular.cfg\"\nsed -i \"s|^import_path = .*|import_path = $3|\" \"$1/modular.cfg\"";

struct MojoExtension;

#[derive(Debug, PartialEq, Eq)]
struct CommandSpec {
    command: String,
    args: Vec<String>,
    envs: Vec<(String, String)>,
}

#[derive(Debug, PartialEq, Eq)]
struct DebugAdapterSpec {
    command: String,
    envs: Vec<(String, String)>,
    plugin_path: Option<String>,
    visualizers_path: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct MojoRunTask {
    source: String,
    build_args: Vec<String>,
    program_args: Vec<String>,
}

impl MojoExtension {
    fn has_project_file(worktree: &zed::Worktree, path: &str) -> bool {
        worktree.read_text_file(path).is_ok()
    }

    fn is_pixi_project(worktree: &zed::Worktree) -> bool {
        Self::has_project_file(worktree, "pixi.toml")
            || worktree
                .read_text_file("pyproject.toml")
                .is_ok_and(|contents| {
                    contents.lines().any(|line| {
                        let line = line.trim();
                        !line.starts_with('#') && line.starts_with("[tool.pixi")
                    })
                })
    }

    fn platform_bin_dir() -> &'static str {
        match zed::current_platform().0 {
            zed::Os::Windows => "Scripts",
            _ => "bin",
        }
    }

    fn environment_binary(root: &str, command: &str) -> String {
        let executable_suffix = match zed::current_platform().0 {
            zed::Os::Windows => ".exe",
            _ => "",
        };
        let separator = if root.ends_with('/') || root.ends_with('\\') {
            ""
        } else {
            "/"
        };
        format!(
            "{root}{separator}{bin_dir}/{command}{executable_suffix}",
            bin_dir = Self::platform_bin_dir()
        )
    }

    fn project_binary(
        worktree: &zed::Worktree,
        environment_root: &str,
        command: &str,
    ) -> Option<String> {
        let environment_root = if environment_root.starts_with('/') {
            environment_root.to_string()
        } else {
            format!("{}/{environment_root}", worktree.root_path())
        };
        worktree.which(&Self::environment_binary(&environment_root, command))
    }

    fn library_extension() -> &'static str {
        match zed::current_platform().0 {
            zed::Os::Mac => "dylib",
            _ => "so",
        }
    }

    fn path_separator() -> &'static str {
        match zed::current_platform().0 {
            zed::Os::Windows => ";",
            _ => ":",
        }
    }

    fn derived_pixi_debug_paths(worktree: &zed::Worktree) -> (Option<String>, Option<String>) {
        let sdk_root = format!("{}/.pixi/envs/default", worktree.root_path());
        let plugin = worktree.which(&format!(
            "{sdk_root}/lib/libMojoLLDB.{extension}",
            extension = Self::library_extension()
        ));
        let visualizers = worktree
            .which(&format!(
                "{sdk_root}/lib/lldb-visualizers/lldbDataFormatters.py"
            ))
            .map(|_| format!("{sdk_root}/lib/lldb-visualizers"));
        (plugin, visualizers)
    }

    fn pixi_sdk_env(worktree: &zed::Worktree) -> Vec<(String, String)> {
        let sdk_root = format!("{}/.pixi/envs/default", worktree.root_path());
        let sdk_bin = format!("{sdk_root}/{}", Self::platform_bin_dir());
        let path = Self::shell_env_value(worktree, "PATH")
            .filter(|path| !path.is_empty())
            .map(|path| format!("{sdk_bin}{}{path}", Self::path_separator()))
            .unwrap_or(sdk_bin);
        let mut envs = vec![
            ("MODULAR_HOME".into(), format!("{sdk_root}/share/max")),
            ("CONDA_PREFIX".into(), sdk_root),
            ("PATH".into(), path),
        ];
        let config = worktree
            .read_text_file(".pixi/envs/default/share/max/modular.cfg")
            .ok();
        let configured = |key| {
            config
                .as_deref()
                .and_then(|contents| Self::config_value(contents, "mojo-max", key))
        };
        let (derived_plugin, derived_visualizers) = Self::derived_pixi_debug_paths(worktree);
        let configured_plugin =
            configured("lldb_plugin_path").and_then(|path| worktree.which(&path));
        let configured_visualizers = configured("lldb_visualizers_path").and_then(|path| {
            worktree
                .which(&format!("{path}/lldbDataFormatters.py"))
                .map(|_| path)
        });
        for (value, variable) in [
            (
                configured_plugin.or(derived_plugin),
                "MODULAR_MOJO_MAX_LLDB_PLUGIN_PATH",
            ),
            (
                configured_visualizers.or(derived_visualizers),
                "MODULAR_MOJO_MAX_LLDB_VISUALIZERS_PATH",
            ),
        ] {
            if let Some(value) = value {
                envs.push((variable.into(), value));
            }
        }
        envs
    }

    fn venv_sdk_env(worktree: &zed::Worktree) -> Vec<(String, String)> {
        let Some(version) = Self::python_version(worktree) else {
            return Vec::new();
        };
        let modular_lib = format!(
            "{}/.venv/lib/python{version}/site-packages/modular/lib",
            worktree.root_path()
        );
        vec![
            (
                "MODULAR_MOJO_MAX_LLDB_PLUGIN_PATH".into(),
                format!(
                    "{modular_lib}/libMojoLLDB.{extension}",
                    extension = Self::library_extension()
                ),
            ),
            (
                "MODULAR_MOJO_MAX_LLDB_VISUALIZERS_PATH".into(),
                format!("{modular_lib}/lldb-visualizers"),
            ),
        ]
    }

    /// Locate Mojo tooling without letting an unrelated global environment
    /// manager hijack ordinary projects.
    fn find_command(worktree: &zed::Worktree, command: &str) -> Option<CommandSpec> {
        // A declared project environment takes precedence over an unrelated
        // global SDK, so the language server and compiler match the lockfile.
        if Self::is_pixi_project(worktree)
            && let Some(command) = Self::project_binary(worktree, ".pixi/envs/default", command)
        {
            return Some(CommandSpec {
                command,
                args: Vec::new(),
                envs: Self::pixi_sdk_env(worktree),
            });
        }

        if let Some(command) = Self::project_binary(worktree, ".venv", command) {
            return Some(CommandSpec {
                command,
                args: Vec::new(),
                envs: Self::venv_sdk_env(worktree),
            });
        }

        // Activated Mojo/MAX environments, including Conda, and standalone
        // installations are resolved through the worktree shell PATH.
        if let Some(path) = worktree.which(command) {
            return Some(CommandSpec {
                command: path,
                args: Vec::new(),
                envs: Vec::new(),
            });
        }

        None
    }

    fn config_value(contents: &str, section: &str, key: &str) -> Option<String> {
        let mut current_section = "";
        for raw_line in contents.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with(['#', ';']) {
                continue;
            }
            if let Some(section_name) = line
                .strip_prefix('[')
                .and_then(|line| line.strip_suffix(']'))
            {
                current_section = section_name.trim();
                continue;
            }
            if current_section != section {
                continue;
            }
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            if name.trim() == key {
                return Some(value.trim().trim_matches(['"', '\'']).to_string());
            }
        }
        None
    }

    fn pixi_debug_adapter(worktree: &zed::Worktree) -> Option<DebugAdapterSpec> {
        if !Self::is_pixi_project(worktree) {
            return None;
        }
        let config = worktree
            .read_text_file(".pixi/envs/default/share/max/modular.cfg")
            .ok();
        let configured = |key| {
            config
                .as_deref()
                .and_then(|contents| Self::config_value(contents, "mojo-max", key))
        };
        let (derived_plugin, derived_visualizers) = Self::derived_pixi_debug_paths(worktree);
        let plugin_path = configured("lldb_plugin_path")
            .and_then(|path| worktree.which(&path))
            .or(derived_plugin)?;
        let visualizers_path = configured("lldb_visualizers_path")
            .and_then(|path| {
                worktree
                    .which(&format!("{path}/lldbDataFormatters.py"))
                    .map(|_| path)
            })
            .or(derived_visualizers);
        let configured_command =
            configured("lldb_vscode_path").and_then(|path| worktree.which(&path));
        let legacy_command =
            Self::project_binary(worktree, ".pixi/envs/default", LEGACY_DAP_BINARY);
        let plain_command = Self::project_binary(worktree, ".pixi/envs/default", "lldb-dap");
        // Current Mojo SDK packages expose `mojo-lldb-dap`, which relies on
        // CONDA_PREFIX and imports the SDK visualizers. Prefer it to a plain
        // lldb-dap, which is only safe once the matching Mojo plugin exists.
        let command = configured_command.or(legacy_command).or(plain_command)?;
        let mut spec = Self::debug_adapter_spec(command, Some(plugin_path), visualizers_path);
        spec.envs = Self::pixi_sdk_env(worktree);
        Some(spec)
    }

    fn python_version(worktree: &zed::Worktree) -> Option<String> {
        worktree
            .read_text_file(".venv/pyvenv.cfg")
            .ok()?
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once('=')?;
                if name.trim() != "version" {
                    return None;
                }
                let mut components = value.trim().split('.');
                Some(format!("{}.{}", components.next()?, components.next()?))
            })
    }

    fn venv_debug_adapter(worktree: &zed::Worktree) -> Option<DebugAdapterSpec> {
        let command = Self::project_binary(worktree, ".venv", "lldb-dap")?;
        let envs = Self::venv_sdk_env(worktree);
        let plugin_path = envs
            .iter()
            .find(|(key, _)| key == "MODULAR_MOJO_MAX_LLDB_PLUGIN_PATH")
            .map(|(_, value)| value.clone());
        let visualizers_path = envs
            .iter()
            .find(|(key, _)| key == "MODULAR_MOJO_MAX_LLDB_VISUALIZERS_PATH")
            .map(|(_, value)| value.clone());
        let mut spec = Self::debug_adapter_spec(command, plugin_path, visualizers_path);
        spec.envs = envs;
        Some(spec)
    }

    fn debug_adapter_spec(
        command: String,
        plugin_path: Option<String>,
        visualizers_path: Option<String>,
    ) -> DebugAdapterSpec {
        let mut envs = Vec::new();
        if let Some(path) = plugin_path.as_ref() {
            envs.push(("MODULAR_MOJO_MAX_LLDB_PLUGIN_PATH".into(), path.clone()));
        }
        if let Some(path) = visualizers_path.as_ref() {
            envs.push((
                "MODULAR_MOJO_MAX_LLDB_VISUALIZERS_PATH".into(),
                path.clone(),
            ));
        }
        DebugAdapterSpec {
            command,
            envs,
            plugin_path,
            visualizers_path,
        }
    }

    fn shell_env_value(worktree: &zed::Worktree, key: &str) -> Option<String> {
        worktree
            .shell_env()
            .into_iter()
            .find_map(|(name, value)| (name == key).then_some(value))
    }

    fn env_value(envs: &[(String, String)], key: &str) -> Option<String> {
        envs.iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
    }

    fn resolve_worktree_path(worktree: &zed::Worktree, path: &str) -> String {
        if path.starts_with('/') {
            path.to_string()
        } else {
            format!("{}/{}", worktree.root_path(), path.trim_matches('/'))
        }
    }

    fn stdlib_source_setting(worktree: &zed::Worktree) -> Option<String> {
        let settings = LspSettings::for_worktree(LANGUAGE_SERVER_ID, worktree).ok()?;
        settings
            .settings
            .as_ref()
            .and_then(|settings| settings.get("stdlib_source"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|source| !source.is_empty())
    }

    /// Route the LSP's built-in `std` imports to a stdlib source tree.
    ///
    /// The SDK resolves `std` from the compiled package named as `import_path`
    /// in `$MODULAR_HOME/modular.cfg`; `-I` cannot override it because the
    /// built-in package shadows user search paths. A shadow SDK home whose
    /// config points `import_path` at the source tree makes definitions
    /// resolve there instead.
    fn apply_stdlib_source(
        worktree: &zed::Worktree,
        spec: &mut CommandSpec,
        stdlib_source: &str,
    ) -> Result<()> {
        let import_path = Self::resolve_worktree_path(worktree, stdlib_source);
        let home = Self::env_value(&spec.envs, "MODULAR_HOME")
            .or_else(|| Self::shell_env_value(worktree, "MODULAR_HOME"))
            .ok_or_else(|| {
                "stdlib_source needs a detected SDK home (MODULAR_HOME); use a root Pixi project or activate the SDK environment".to_string()
            })?;
        // The SDK cfg is read by the shadow-home script at the filesystem
        // level: worktree.read_text_file cannot see gitignored paths such as
        // `.pixi/...` (no worktree entry), but the SDK may live anywhere.

        let directory = Self::temp_directory("the stdlib-source SDK home")?;
        Self::run_process(
            ProcessCommand::new("/bin/sh").args([
                "-c",
                STDLIB_HOME_SCRIPT,
                "zed-mojo-stdlib-home",
                directory.as_str(),
                home.as_str(),
                import_path.as_str(),
            ]),
            "writing the stdlib-source shadow SDK home",
        )?;
        match spec
            .envs
            .iter_mut()
            .find(|(name, _)| name == "MODULAR_HOME")
        {
            Some((_, value)) => *value = directory,
            None => spec.envs.push(("MODULAR_HOME".into(), directory)),
        }
        Ok(())
    }

    fn find_debug_adapter(worktree: &zed::Worktree) -> Option<DebugAdapterSpec> {
        // Prefer the current project SDK. A system lldb-dap does not load the
        // Mojo language plugin and must not supersede it.
        if let Some(spec) = Self::pixi_debug_adapter(worktree) {
            return Some(spec);
        }
        if let Some(spec) = Self::venv_debug_adapter(worktree) {
            return Some(spec);
        }

        // Current wheel/activated-environment installs expose both executables
        // from the same bin directory. Derive lldb-dap from Mojo rather than
        // accepting an unrelated global LLDB. A direct adapter also requires
        // a discoverable Mojo plugin; otherwise prefer the legacy wrapper.
        if let Some(mojo) = worktree.which("mojo") {
            let bin_dir = mojo
                .rfind(['/', '\\'])
                .map(|index| &mojo[..index])
                .unwrap_or_default();
            let plugin_path = Self::shell_env_value(worktree, "MODULAR_MOJO_MAX_LLDB_PLUGIN_PATH");
            let visualizers_path =
                Self::shell_env_value(worktree, "MODULAR_MOJO_MAX_LLDB_VISUALIZERS_PATH");
            let dap = worktree.which(&format!("{bin_dir}/lldb-dap"));
            if plugin_path.is_some()
                && let Some(dap) = dap
            {
                return Some(Self::debug_adapter_spec(dap, plugin_path, visualizers_path));
            }
        }

        worktree
            .which(LEGACY_DAP_BINARY)
            .map(|path| Self::debug_adapter_spec(path, None, None))
    }

    fn command_name(command: &str) -> &str {
        let name = command.rsplit(['/', '\\']).next().unwrap_or(command);
        name.strip_suffix(".exe").unwrap_or(name)
    }

    fn task_source_argument(argument: &str) -> Option<String> {
        let argument = argument
            .strip_prefix('"')
            .and_then(|argument| argument.strip_suffix('"'))
            .unwrap_or(argument);
        (matches!(
            argument,
            "$ZED_FILE" | "${ZED_FILE}" | "$ZED_RELATIVE_FILE" | "${ZED_RELATIVE_FILE}"
        ) || Self::is_mojo_source(argument))
        .then(|| argument.to_string())
    }

    fn task_mojo_run_args(task: &TaskTemplate) -> Option<&[String]> {
        let command = Self::command_name(&task.command);
        let prefix: &[&str] = match command {
            "mojo" => &["run"],
            "pixi" => &[
                "run",
                "--frozen",
                "--no-progress",
                "--executable",
                "mojo",
                "run",
            ],
            "uv" => &["run", "--frozen", "mojo", "run"],
            _ => return None,
        };
        if task.args.len() < prefix.len()
            || !task
                .args
                .iter()
                .zip(prefix)
                .all(|(argument, expected)| argument == expected)
        {
            return None;
        }
        Some(&task.args[prefix.len()..])
    }

    fn is_zed_file_variable(argument: &str) -> bool {
        matches!(
            argument.trim_matches('"'),
            "$ZED_FILE" | "${ZED_FILE}" | "$ZED_RELATIVE_FILE" | "${ZED_RELATIVE_FILE}"
        )
    }

    fn debug_build_arguments(arguments: &[String]) -> Vec<String> {
        let mut filtered = Vec::new();
        let mut arguments = arguments.iter();
        while let Some(argument) = arguments.next() {
            if matches!(
                argument.as_str(),
                "-O" | "--optimization-level"
                    | "-optimization-level"
                    | "--debug-level"
                    | "-debug-level"
            ) {
                // These options consume their value as the next argument.
                arguments.next();
                continue;
            }
            if matches!(argument.as_str(), "--no-optimization" | "-no-optimization")
                || argument.starts_with("--optimization-level=")
                || argument.starts_with("-optimization-level=")
                || argument.starts_with("--debug-level=")
                || argument.starts_with("-debug-level=")
                || (argument.starts_with("-O") && argument.len() > 2)
                || argument.starts_with("-g")
            {
                continue;
            }
            filtered.push(argument.clone());
        }
        filtered
    }

    fn task_mojo_run(task: &TaskTemplate) -> Option<MojoRunTask> {
        let args = Self::task_mojo_run_args(task)?;
        // Zed variables identify the source unambiguously, even if a compiler
        // option happens to contain another path ending in `.mojo`.
        let source = args
            .iter()
            .position(|argument| Self::is_zed_file_variable(argument))
            .or_else(|| {
                args.iter()
                    .position(|argument| Self::is_mojo_source(argument.trim_matches('"')))
            })?;
        Some(MojoRunTask {
            source: Self::task_source_argument(&args[source])?,
            build_args: Self::debug_build_arguments(&args[..source]),
            program_args: args[source + 1..].to_vec(),
        })
    }

    fn quoted_python_string(value: &str) -> String {
        serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
    }

    fn quoted_lldb_path(value: &str) -> String {
        value.replace('\\', "\\\\").replace('\'', "\\'")
    }

    fn configure_mojo_debugger(config: &mut Value, spec: &DebugAdapterSpec) -> Result<()> {
        let map = config
            .as_object_mut()
            .ok_or_else(|| "Mojo debug configuration must be a JSON object".to_string())?;

        map.entry("enableSyntheticChildDebugging")
            .or_insert(Value::Bool(true));
        map.entry("enableAutoVariableSummaries")
            .or_insert(Value::Bool(true));
        map.entry("commandEscapePrefix")
            .or_insert(Value::String(":".into()));

        let mut commands = Vec::new();
        if let Some(plugin_path) = spec.plugin_path.as_deref() {
            let plugin_path = Self::quoted_lldb_path(plugin_path);
            commands.push(Value::String(format!("?!plugin load '{plugin_path}'")));
        }
        commands.push(Value::String(
            "?settings set target.show-hex-variable-values-with-leading-zeroes false".into(),
        ));
        commands.push(Value::String(
            "?settings set target.process.optimization-warnings false".into(),
        ));
        if let Some(visualizers_path) = spec.visualizers_path.as_deref() {
            let path = Self::quoted_python_string(visualizers_path);
            commands.push(Value::String(format!(
                "?script import glob, lldb; [lldb.debugger.HandleCommand('command script import ' + repr(file)) for file in glob.glob({path} + '/*')]"
            )));
        }
        if let Some(existing) = map.remove("initCommands") {
            let existing = existing.as_array().ok_or_else(|| {
                "Mojo debug `initCommands` must be an array of strings".to_string()
            })?;
            commands.extend(existing.iter().cloned());
        }
        map.insert("initCommands".into(), Value::Array(commands));
        Ok(())
    }

    fn build_arguments(config: &Value) -> Result<Vec<String>> {
        let arguments = match config.get("buildArgs") {
            None => Ok(Vec::new()),
            Some(Value::String(argument)) => Ok(vec![argument.clone()]),
            Some(Value::Array(arguments)) => arguments
                .iter()
                .map(|argument| {
                    argument
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| "Every Mojo `buildArgs` entry must be a string".to_string())
                })
                .collect(),
            Some(_) => Err("Mojo `buildArgs` must be a string or array of strings".into()),
        }?;
        Ok(Self::debug_build_arguments(&arguments))
    }

    fn config_environment(config: &Value) -> Result<Vec<(String, String)>> {
        match config.get("env") {
            None => Ok(Vec::new()),
            Some(Value::Object(environment)) => environment
                .iter()
                .map(|(name, value)| {
                    value
                        .as_str()
                        .map(|value| (name.clone(), value.to_string()))
                        .ok_or_else(|| "Every Mojo debug `env` value must be a string".to_string())
                })
                .collect(),
            Some(_) => Err("Mojo debug `env` must be an object of string values".into()),
        }
    }

    fn is_mojo_source(path: &str) -> bool {
        path.ends_with(".mojo") || path.ends_with(".🔥")
    }

    fn temp_directory(purpose: &str) -> Result<String> {
        let mut command =
            ProcessCommand::new("/usr/bin/mktemp").args(["-d", "/tmp/zed-mojo.XXXXXXXXXX"]);
        let output = command.output()?;
        if output.status != Some(0) {
            return Err(format!(
                "creating a temporary directory for {purpose} failed with status {:?}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let directory = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !directory.starts_with("/tmp/zed-mojo.") || directory.contains(['\n', '\r']) {
            return Err(format!(
                "`mktemp` returned an unexpected directory for {purpose}"
            ));
        }
        Ok(directory)
    }

    fn debug_binary_path(source: &str) -> Result<String> {
        let directory = Self::temp_directory("the Mojo debug binary")?;
        let stem = source
            .rsplit(['/', '\\'])
            .next()
            .map(|name| {
                name.strip_suffix(".mojo")
                    .or_else(|| name.strip_suffix(".🔥"))
                    .unwrap_or(name)
            })
            .filter(|stem| !stem.is_empty())
            .unwrap_or("program");
        Ok(format!("{directory}/{stem}"))
    }

    fn run_process(mut command: ProcessCommand, purpose: &str) -> Result<()> {
        let output = command.output()?;
        if output.status == Some(0) {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "{purpose} failed with status {:?}: {}{}",
            output.status,
            stderr.trim(),
            if stderr.is_empty() { stdout.trim() } else { "" }
        ))
    }

    fn codesign_debug_binary(binary: &str) -> Result<()> {
        if zed::current_platform().0 != zed::Os::Mac {
            return Ok(());
        }
        Self::run_process(
            ProcessCommand::new("/bin/sh").args([
                "-c",
                CODESIGN_SCRIPT,
                "zed-mojo-codesign",
                binary,
            ]),
            "codesigning the Mojo debug binary",
        )
    }

    fn build_cwd(worktree: &zed::Worktree, config: &Value) -> Result<String> {
        match config.get("cwd") {
            None => Ok(worktree.root_path()),
            Some(Value::String(cwd)) if cwd.is_empty() => Ok(worktree.root_path()),
            Some(Value::String(cwd)) if cwd.starts_with('/') => Ok(cwd.clone()),
            Some(Value::String(cwd)) => Ok(format!("{}/{cwd}", worktree.root_path())),
            Some(_) => Err("Mojo debug `cwd` must be a string".into()),
        }
    }

    fn resolve_source_debug(worktree: &zed::Worktree, config: &mut Value) -> Result<()> {
        let Some(source) = config
            .get("mojoFile")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return Ok(());
        };
        if !Self::is_mojo_source(&source) {
            return Err("`mojoFile` must point to a `.mojo` or `.🔥` source file".into());
        }

        let mojo = Self::find_command(worktree, "mojo").ok_or_else(|| {
            "The Mojo compiler was not found for source debugging; activate an SDK or configure a project Pixi/uv environment".to_string()
        })?;
        let build_cwd = Self::build_cwd(worktree, config)?;
        let build_arguments = Self::build_arguments(config)?;
        let config_environment = Self::config_environment(config)?;
        let binary = Self::debug_binary_path(&source)?;
        let mut args = vec![
            "build".into(),
            "--no-optimization".into(),
            "--debug-level".into(),
            "full".into(),
        ];
        args.extend(build_arguments);
        args.extend([source, "-o".into(), binary.clone()]);
        let mut envs = worktree.shell_env();
        envs.extend(mojo.envs);
        // Match the compilation environment to the debuggee environment.
        // Task/config values deliberately win over the detected SDK defaults.
        envs.extend(config_environment);
        Self::run_process(
            ProcessCommand::new("/bin/sh")
                .args(
                    [
                        vec![
                            "-c".into(),
                            BUILD_SCRIPT.into(),
                            "zed-mojo-build".into(),
                            build_cwd,
                            mojo.command,
                        ],
                        args,
                    ]
                    .concat(),
                )
                .envs(envs),
            "building the Mojo debug target",
        )?;
        Self::codesign_debug_binary(&binary)?;

        let map = config
            .as_object_mut()
            .ok_or_else(|| "Mojo debug configuration must be a JSON object".to_string())?;
        map.remove("mojoFile");
        map.remove("buildArgs");
        map.insert("program".into(), Value::String(binary));
        Ok(())
    }

    fn request_kind(config: &Value) -> Result<StartDebuggingRequestArgumentsRequest, String> {
        match config.get("request").and_then(Value::as_str) {
            Some("launch") => Ok(StartDebuggingRequestArgumentsRequest::Launch),
            Some("attach") => Ok(StartDebuggingRequestArgumentsRequest::Attach),
            _ => Err("Invalid request: expected `request` to be `launch` or `attach`".into()),
        }
    }

    fn validate_language_server(language_server_id: &zed::LanguageServerId) -> Result<()> {
        if language_server_id.as_ref() == LANGUAGE_SERVER_ID {
            Ok(())
        } else {
            Err(format!(
                "Mojo extension does not support language server `{language_server_id}`"
            ))
        }
    }

    fn validate_debug_adapter(adapter_name: &str) -> Result<(), String> {
        if adapter_name == DEBUG_ADAPTER_ID {
            Ok(())
        } else {
            Err(format!(
                "Mojo extension does not support debug adapter `{adapter_name}` (supported: `{DEBUG_ADAPTER_ID}`)"
            ))
        }
    }
}

impl zed::Extension for MojoExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        Self::validate_language_server(language_server_id)?;
        let Some(mut spec) = Self::find_command(worktree, LSP_BINARY) else {
            return Err(format!(
                "`{LSP_BINARY}` was not found. Install Mojo/MAX and activate it on PATH, use a root Pixi/uv worktree, or configure `lsp.{LANGUAGE_SERVER_ID}.binary.path`."
            ));
        };
        if let Some(stdlib_source) = Self::stdlib_source_setting(worktree)
            && let Err(error) = Self::apply_stdlib_source(worktree, &mut spec, &stdlib_source)
        {
            // An explicitly configured stdlib_source that cannot be applied
            // is a misconfiguration: fail loudly (Zed surfaces the error and
            // shows the reason) instead of silently dead-ending definitions
            // on the compiled std.mojoc.
            return Err(format!("stdlib_source `{stdlib_source}`: {error}"));
        }
        Ok(zed::Command {
            command: spec.command,
            args: spec.args,
            env: spec.envs,
        })
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<Value>> {
        Self::validate_language_server(language_server_id)?;
        LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .map(|settings| settings.initialization_options)
    }

    fn language_server_workspace_configuration(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<Value>> {
        Self::validate_language_server(language_server_id)?;
        LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .map(|settings| settings.settings)
    }

    fn get_dap_binary(
        &mut self,
        adapter_name: String,
        config: DebugTaskDefinition,
        user_provided_debug_adapter_path: Option<String>,
        worktree: &zed::Worktree,
    ) -> Result<DebugAdapterBinary, String> {
        Self::validate_debug_adapter(&adapter_name)?;
        let mut parsed_config: Value = serde_json::from_str(&config.config)
            .map_err(|error| format!("Invalid Mojo debug configuration: {error}"))?;
        let request = Self::request_kind(&parsed_config)?;
        let connection = config
            .tcp_connection
            .map(zed::resolve_tcp_template)
            .transpose()?;

        if connection.is_some() {
            if parsed_config.get("mojoFile").is_some() {
                return Err("`mojoFile` cannot be built for an external TCP adapter; build a `program` that is accessible to the adapter".into());
            }
            return Ok(DebugAdapterBinary {
                command: None,
                arguments: Vec::new(),
                envs: Vec::new(),
                cwd: Some(worktree.root_path()),
                connection,
                request_args: StartDebuggingRequestArguments {
                    configuration: parsed_config.to_string(),
                    request,
                },
            });
        }

        if request == StartDebuggingRequestArgumentsRequest::Launch {
            Self::resolve_source_debug(worktree, &mut parsed_config)?;
        }
        let discovered_spec = Self::find_debug_adapter(worktree);

        let spec = if let Some(path) = user_provided_debug_adapter_path {
            let mut spec = discovered_spec
                .unwrap_or_else(|| Self::debug_adapter_spec(path.clone(), None, None));
            spec.command = path;
            spec
        } else {
            discovered_spec.ok_or_else(|| {
                "A Mojo SDK debugger was not found. Activate a Mojo/MAX environment, use a root Pixi/.venv worktree, or configure a debug adapter path.".to_string()
            })?
        };
        Self::configure_mojo_debugger(&mut parsed_config, &spec)?;

        Ok(DebugAdapterBinary {
            command: Some(spec.command),
            arguments: vec!["--repl-mode".into(), "variable".into()],
            envs: spec.envs,
            cwd: Some(worktree.root_path()),
            connection,
            request_args: StartDebuggingRequestArguments {
                configuration: parsed_config.to_string(),
                request,
            },
        })
    }

    fn dap_request_kind(
        &mut self,
        adapter_name: String,
        config: Value,
    ) -> Result<StartDebuggingRequestArgumentsRequest, String> {
        Self::validate_debug_adapter(&adapter_name)?;
        Self::request_kind(&config)
    }

    fn dap_config_to_scenario(&mut self, config: DebugConfig) -> Result<DebugScenario, String> {
        Self::validate_debug_adapter(&config.adapter)?;

        let mut configuration = serde_json::json!({
            "request": match config.request {
                DebugRequest::Launch(_) => "launch",
                DebugRequest::Attach(_) => "attach",
            },
        });
        let map = configuration
            .as_object_mut()
            .ok_or_else(|| "Failed to construct Mojo debug configuration".to_string())?;

        match &config.request {
            DebugRequest::Attach(attach) => {
                if let Some(pid) = attach.process_id {
                    map.insert("pid".into(), pid.into());
                }
            }
            DebugRequest::Launch(launch) => {
                if !launch.program.is_empty() {
                    let key = if Self::is_mojo_source(&launch.program) {
                        "mojoFile"
                    } else {
                        "program"
                    };
                    map.insert(key.into(), launch.program.clone().into());
                }
                if !launch.args.is_empty() {
                    map.insert("args".into(), launch.args.clone().into());
                }
                if !launch.envs.is_empty() {
                    map.insert(
                        "env".into(),
                        Value::Object(
                            launch
                                .envs
                                .clone()
                                .into_iter()
                                .map(|(key, value)| (key, Value::String(value)))
                                .collect(),
                        ),
                    );
                }
                if let Some(stop_on_entry) = config.stop_on_entry {
                    map.insert("stopOnEntry".into(), stop_on_entry.into());
                }
                if let Some(cwd) = &launch.cwd {
                    map.insert("cwd".into(), cwd.to_string().into());
                }
            }
        }

        Ok(DebugScenario {
            adapter: config.adapter,
            label: config.label,
            config: configuration.to_string(),
            build: None,
            tcp_connection: None,
        })
    }

    fn dap_locator_create_scenario(
        &mut self,
        locator_name: String,
        task: TaskTemplate,
        resolved_label: String,
        debug_adapter_name: String,
    ) -> Option<DebugScenario> {
        if locator_name != DEBUG_LOCATOR_ID || debug_adapter_name != DEBUG_ADAPTER_ID {
            return None;
        }
        let run = Self::task_mojo_run(&task)?;
        let mut config = serde_json::json!({
            "request": "launch",
            "mojoFile": run.source,
        });
        let map = config.as_object_mut()?;
        if !run.build_args.is_empty() {
            map.insert("buildArgs".into(), run.build_args.into());
        }
        if !run.program_args.is_empty() {
            map.insert("args".into(), run.program_args.into());
        }
        if let Some(cwd) = task.cwd {
            map.insert("cwd".into(), cwd.into());
        }
        if !task.env.is_empty() {
            map.insert(
                "env".into(),
                Value::Object(
                    task.env
                        .into_iter()
                        .map(|(key, value)| (key, Value::String(value)))
                        .collect(),
                ),
            );
        }
        Some(DebugScenario {
            adapter: debug_adapter_name,
            label: resolved_label,
            config: config.to_string(),
            build: None,
            tcp_connection: None,
        })
    }
}

zed::register_extension!(MojoExtension);

#[cfg(test)]
mod tests {
    use super::*;
    use zed::Extension as _;

    #[test]
    fn parses_launch_and_attach_requests() {
        assert_eq!(
            MojoExtension::request_kind(&serde_json::json!({ "request": "launch" })),
            Ok(StartDebuggingRequestArgumentsRequest::Launch)
        );
        assert_eq!(
            MojoExtension::request_kind(&serde_json::json!({ "request": "attach" })),
            Ok(StartDebuggingRequestArgumentsRequest::Attach)
        );
    }

    #[test]
    fn rejects_missing_or_unknown_requests() {
        assert!(MojoExtension::request_kind(&serde_json::json!({})).is_err());
        assert!(MojoExtension::request_kind(&serde_json::json!({ "request": "restart" })).is_err());
        assert!(MojoExtension::request_kind(&serde_json::json!({ "request": 1 })).is_err());
    }

    #[test]
    fn parses_modular_config_values() {
        let config = r#"
            [max]
            version = 1.0

            [mojo-max]
            lldb_vscode_path = "/sdk/bin/lldb-dap"
            lldb_plugin_path='/sdk/lib/libMojoLLDB.so'
        "#;
        assert_eq!(
            MojoExtension::config_value(config, "mojo-max", "lldb_vscode_path"),
            Some("/sdk/bin/lldb-dap".into())
        );
        assert_eq!(
            MojoExtension::config_value(config, "mojo-max", "lldb_plugin_path"),
            Some("/sdk/lib/libMojoLLDB.so".into())
        );
        assert_eq!(
            MojoExtension::config_value(config, "max", "lldb_plugin_path"),
            None
        );
    }

    #[test]
    fn validates_build_arguments() {
        assert_eq!(
            MojoExtension::build_arguments(&serde_json::json!({ "buildArgs": "-Ilib" })),
            Ok(vec!["-Ilib".into()])
        );
        assert_eq!(
            MojoExtension::build_arguments(&serde_json::json!({ "buildArgs": ["-I", "lib"] })),
            Ok(vec!["-I".into(), "lib".into()])
        );
        assert_eq!(
            MojoExtension::build_arguments(&serde_json::json!({
                "buildArgs": [
                    "-O", "1", "-O3", "--optimization-level", "2",
                    "--optimization-level=1", "--no-optimization",
                    "-g", "-g0", "--debug-level", "line-tables",
                    "--debug-level=none", "-optimization-level=3",
                    "-debug-level", "none", "-no-optimization", "-I", "lib"
                ]
            })),
            Ok(vec!["-I".into(), "lib".into()])
        );
        assert!(MojoExtension::build_arguments(&serde_json::json!({ "buildArgs": [1] })).is_err());
    }

    #[test]
    fn validates_debug_environment() {
        assert_eq!(
            MojoExtension::config_environment(
                &serde_json::json!({ "env": { "MOJO_ENABLE_ASSERTIONS": "1" } })
            ),
            Ok(vec![("MOJO_ENABLE_ASSERTIONS".into(), "1".into())])
        );
        assert!(MojoExtension::config_environment(&serde_json::json!({ "env": [] })).is_err());
        assert!(
            MojoExtension::config_environment(&serde_json::json!({ "env": { "KEY": 1 } })).is_err()
        );
    }

    #[test]
    fn configures_plugin_before_user_commands() {
        let spec = MojoExtension::debug_adapter_spec(
            "/sdk/bin/lldb-dap".into(),
            Some("/sdk/lib/libMojoLLDB.so".into()),
            Some("/sdk/lib/lldb-visualizers".into()),
        );
        let mut config = serde_json::json!({
            "request": "launch",
            "program": "/tmp/program",
            "initCommands": ["settings set stop-disassembly-count 0"]
        });
        MojoExtension::configure_mojo_debugger(&mut config, &spec).unwrap();

        let commands = config["initCommands"].as_array().unwrap();
        assert!(commands[0].as_str().unwrap().contains("plugin load"));
        assert_eq!(
            commands.last().and_then(Value::as_str),
            Some("settings set stop-disassembly-count 0")
        );
        assert_eq!(config["commandEscapePrefix"], ":");
        assert_eq!(config["enableSyntheticChildDebugging"], true);
    }

    #[test]
    fn recognizes_current_mojo_source_suffixes() {
        assert!(MojoExtension::is_mojo_source("main.mojo"));
        assert!(MojoExtension::is_mojo_source("main.🔥"));
        assert!(!MojoExtension::is_mojo_source("main"));
    }

    #[test]
    fn extracts_mojo_run_tasks_for_debugging() {
        let task = |command: &str, args: &[&str]| TaskTemplate {
            label: "run".into(),
            command: command.into(),
            args: args.iter().map(|arg| (*arg).into()).collect(),
            env: Vec::new(),
            cwd: None,
        };
        assert_eq!(
            MojoExtension::task_mojo_run(&task("mojo", &["run", "main.mojo"])),
            Some(MojoRunTask {
                source: "main.mojo".into(),
                build_args: Vec::new(),
                program_args: Vec::new(),
            })
        );
        assert_eq!(
            MojoExtension::task_mojo_run(&task(
                "pixi",
                &[
                    "run",
                    "--frozen",
                    "--no-progress",
                    "--executable",
                    "mojo",
                    "run",
                    "-O3",
                    "--debug-level",
                    "none",
                    "-I",
                    "/project/package",
                    "$ZED_RELATIVE_FILE",
                    "--only",
                    "selected_test",
                ],
            )),
            Some(MojoRunTask {
                source: "$ZED_RELATIVE_FILE".into(),
                build_args: vec!["-I".into(), "/project/package".into()],
                program_args: vec!["--only".into(), "selected_test".into()],
            })
        );
        assert_eq!(
            MojoExtension::task_mojo_run(&task(
                "/usr/local/bin/uv",
                &[
                    "run",
                    "--frozen",
                    "mojo",
                    "run",
                    "-Ilib",
                    "main.🔥",
                    "first",
                    "--flag",
                ],
            )),
            Some(MojoRunTask {
                source: "main.🔥".into(),
                build_args: vec!["-Ilib".into()],
                program_args: vec!["first".into(), "--flag".into()],
            })
        );
        assert_eq!(
            MojoExtension::task_mojo_run(&task(
                "/opt/mojo/bin/mojo.exe",
                &["run", "-I", "looks-like.mojo", "$ZED_FILE"],
            )),
            Some(MojoRunTask {
                source: "$ZED_FILE".into(),
                build_args: vec!["-I".into(), "looks-like.mojo".into()],
                program_args: Vec::new(),
            })
        );
        assert!(
            MojoExtension::task_mojo_run(&task("python", &["mojo", "run", "main.mojo"])).is_none()
        );
        assert!(
            MojoExtension::task_mojo_run(&task(
                "pixi",
                &[
                    "run",
                    "--manifest-path",
                    "/project/pixi.toml",
                    "--frozen",
                    "--no-progress",
                    "--executable",
                    "mojo",
                    "run",
                    "main.mojo",
                ],
            ))
            .is_none()
        );
        assert!(
            MojoExtension::task_mojo_run(&task(
                "uv",
                &[
                    "run",
                    "--frozen",
                    "--project",
                    "/project",
                    "mojo",
                    "run",
                    "main.mojo",
                ],
            ))
            .is_none()
        );
        assert!(MojoExtension::task_mojo_run(&task("mojo", &["run", "--help"])).is_none());
    }

    #[test]
    fn locator_preserves_compiler_and_program_arguments() {
        let mut extension = MojoExtension;
        let scenario = extension
            .dap_locator_create_scenario(
                DEBUG_LOCATOR_ID.into(),
                TaskTemplate {
                    label: "run".into(),
                    command: "pixi".into(),
                    args: [
                        "run",
                        "--frozen",
                        "--no-progress",
                        "--executable",
                        "mojo",
                        "run",
                        "-I",
                        "/project/package",
                        "$ZED_FILE",
                        "--mode",
                        "fast",
                    ]
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
                    env: vec![("EXAMPLE".into(), "value".into())],
                    cwd: Some("$ZED_WORKTREE_ROOT".into()),
                },
                "debug".into(),
                DEBUG_ADAPTER_ID.into(),
            )
            .unwrap();
        let config: Value = serde_json::from_str(&scenario.config).unwrap();
        assert_eq!(config["mojoFile"], "$ZED_FILE");
        assert_eq!(
            config["buildArgs"],
            serde_json::json!(["-I", "/project/package"])
        );
        assert_eq!(config["args"], serde_json::json!(["--mode", "fast"]));
        assert_eq!(config["cwd"], "$ZED_WORKTREE_ROOT");
        assert_eq!(config["env"]["EXAMPLE"], "value");
    }
}
