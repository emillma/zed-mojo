comptime DefaultCount[T: Intable, count: Int = 4]: Int = count
comptime Generator[ToWrap: __generator_type[Idx: Int] AnyType] = ToWrap
comptime ThinCallback = def(Int) thin -> Int
comptime MlirType = __mlir_type.`!kgen.type`

__mlir_region passthrough(index: Int):
    return index

def c_identity(value: Int) abi("C") -> Int:
    return value

def borrow(ref value: Int) -> ref[value] Int:
    return value

trait Producer[Output: AnyType]:
    def produce(self) raises Error -> Output:
        ...

@fieldwise_init
struct Counter(Copyable):
    var value: Int

    def __init__(out self, *, deinit move: Self):
        self.value = move.value^

    def update(mut self, imm amount: Int) where amount >= 0:
        self.value += amount

__extension Counter:
    def doubled(self) -> Int:
        return self.value * 2

def apply[T: AnyType](value: T, callback: def(T) capturing -> T) raises:
    var closure = lambda (imm item: T) {imm callback} -> T: callback(item)
    _ = closure(value)

def main() raises:
    var `struct` = 1
    var counter = Counter(1)
    counter.update(2)
    print(counter.doubled())
