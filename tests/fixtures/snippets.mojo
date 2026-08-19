# Syntax fixture covering the default forms emitted by the bundled snippets.

comptime Name = 0

trait ExampleTrait:
    def method(self): ...

struct Example:
    var field: Int

    def __init__(out self):
        self.field = 0

    def __init__(out self, *, copy: Self):
        self.field = copy.field

    def __init__(out self, *, deinit move: Self):
        self.field = move.field^

    def __deinit__(deinit self):
        pass

    def method(self):
        pass

    def mutate(mut self):
        pass

@fieldwise_init
struct Fieldwise:
    var field: Int

def function():
    pass

def raising() raises:
    pass

def test_example() raises:
    pass

def main() raises:
    for item in [1, 2, 3]:
        _ = item
    while False:
        pass
    if True:
        pass
    try:
        raise Error("message")
    except error:
        print(error)
