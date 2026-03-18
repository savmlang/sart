use std::mem::offset_of;
use std::os::raw::c_void;

use crate::structures::QuadPackedData;

pub type DispatchFn = extern "C" fn(*mut CVMTaskState);

/// If the pointer goes NULL
/// You must use the 1st unsigned integer to get the module
/// The second integer represents the `.text` block
#[repr(C)]
pub union Instruction {
  pub module: u64,
  pub fn_: FnInstr,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FnInstr {
  pub arg: u64,
  pub dispatch: DispatchFn,
}

/// This is the program registry state
/// This is created for each thread executed
/// under the Runtime
///
/// PLEASE DO NOT CHANGE THE LAYOUT, IT MAY BREAK
/// 32-bit CPUs benefit from 64-bit alignment as well, not changing alignment
#[repr(C, align(64))]
pub struct VMTaskState {
  // Register Structures (64*8 bits, 64bytes)
  pub r1: QuadPackedData,
  pub r2: QuadPackedData,
  pub r3: QuadPackedData,
  pub r4: QuadPackedData,
  pub r5: QuadPackedData,
  pub r6: QuadPackedData,
  pub r7: QuadPackedData,
  pub r8: QuadPackedData,

  // Part II, pads
  pub scratchpad: *mut QuadPackedData, // scratchpad gives you 24 more registers (192-bytes) [64-byte aligned]
  #[cfg(target_pointer_width = "32")]
  _1: [u8; 4],
  pub largepad: *mut QuadPackedData, // Initially NULL, allocated on request
  #[cfg(target_pointer_width = "32")]
  _2: [u8; 4],

  // --- Hot Path Flags (0-7) ---
  pub flags: u32,
  pub opcode: u32,

  // Instruction pointer in bytecode.
  // Resume data for JIT under JIT-Code
  //
  // Please also note that, under JIT, it is used very differently
  pub curline_or_resume: Packed64,
  // This stores the pointer to the engine
  // But during cooperative async, this is replaced with the pointer of
  // of the AsyncTask (for FFI created async)
  //
  // or, NULL for bytecode defined async
  //
  // This is interpreter's favourite location to embed data
  // in interpretation mode
  pub engine_or_pt: Packed64,
  pub icache_or_to_be_defined: Packed64,

  __reserved: [u8; 8],
}

#[repr(C, align(64))]
pub struct CVMTaskState {
  pub r1: QuadPackedData,
  pub r2: QuadPackedData,
  pub r3: QuadPackedData,
  pub r4: QuadPackedData,
  pub r5: QuadPackedData,
  pub r6: QuadPackedData,
  pub r7: QuadPackedData,
  pub r8: QuadPackedData,

  // Part II, pads
  pub _internal_ptr1: *mut QuadPackedData, // Scratchpad gives you 24 more registers (192-bytes) [64-byte aligned]
  pub largepad: *mut QuadPackedData,       // Initially NULL, allocated on request

  // --- Hot Path Flags (0-7) ---
  pub _internal: [u8; 48],
}

#[repr(C)]
pub union Packed64 {
  pub unsigned: u64,
  pub signed: i64,
  pub usi: usize,
  pub bytes: [u8; 8],
  pub pt: *mut c_void,
}

#[rustfmt::skip]
#[allow(non_snake_case)]
pub mod FLAGS {
  // The function is running under async-aware subsystem
  // It makes the JIT not call the sync poll async method
  pub const FLAG_ASYNC: u32 = 0b000000000000000000000000000000001;

  // This VMTaskState is the 1st task state in the chain
  // This means only this task chain can actually request vm to poll
  pub const FLAG_FIRST: u32 = 0b000000000000000000000000000000010;
}

#[rustfmt::skip]
#[allow(non_snake_case)]
pub mod OPCODES {
  pub const OPCODE_OK: u32 = 0;

  pub const OPCODE_YIELD: u32 = 1;

  pub const OPCODE_SLEEP_MS: u32 = 2;

  // This task has suspended due to a libcall or VM await method
  pub const OPCODE_AWAIT: u32 = 3;

  // This task state has suspended due to a forward recurse
  // This also is called for async SaVM functions since no
  //
  // SaVM function is async without a libcall await
  pub const OPCODE_RECURSE: u32 = 4;
}

#[repr(C, align(64))]
pub struct CVMContext {
  pub _unstable: [u8; 64],
}

const _: () = {
  assert!(size_of::<VMTaskState>() == 128);

  assert!(align_of::<VMTaskState>() == 64);

  assert!(size_of::<CVMTaskState>() == 128);
  assert!(align_of::<CVMTaskState>() == 64);

  // The "Golden Boundary" check
  assert!(offset_of!(VMTaskState, scratchpad) == 64);
};

unsafe impl Send for VMTaskState {}

macro_rules! instruction {
  (
    $(
      $data:expr => $name:ident
    ),*$(,)?
  ) => {
    pastey::paste! {
      $(
        pub const [<INSTRUCTION_ $name:upper>]: u8 = $data;

        #[allow(non_upper_case_globals, dead_code)]
        const [<_FORFIXING_ $data>]: u8 = 0;
      )*

      pub fn parse_instrution(inst: &str) -> Option<u8> {
        match inst {
          $(
            stringify!($name) => {
              Some($data)
            }
          )*
          _ => None
        }
      }
    }
  };
}

// # DANGER
// The VM will happily run undefined code without any form of bounds checking, be advised
//
// All numbers follow Little-Endian standard
//
// ## Register IDS
// - r1 = 0
// - r2 = 1
// - r3 = 2
// - r4 = 3
// - r5 = 4
// - r6 = 5
// - r7 = 6
// - r8 = 7
//
// # Type tag
// - 0: u64
// - 1: u32
// - 2: u16
// - 3: u8
// - 4: i64
// - 5: i32
// - 6: i16
// - 7: i8
// - 8: f64
// - 9: f32
instruction! {
  // This is a multi-purpose SIMD Acceleratable, scalar deoptimizable parallel
  // MOV Operation
  //
  // Please note that this can (and does) also act as `load` operation
  //
  // ## Syntax
  // `vcopy <count tag (1-bit)> <memory flags [future spec] (7bits)> <src flags as u8> <count in u32> <base src1 as i32> <base target2 as i32>`
  //
  // # Count tag
  // - 0: Treat the next as absolute
  // - 1: Get count from r1, the next 32-bits treated as expected count (optimization hint)
  //
  // # Memory Flags
  // [alignment data (2-bits)] # for source 1
  // [alignment data (2-bits)] # for source 2
  // [padding]
  //
  // ## Alignment Data
  // 00: Unknown (Assumes unaligned; goes to max unaligned allowed)
  // 01: 16-bytes (Goes to max AVX)
  // 10: 32-bytes (Goes to max AVX2)
  // 11: 64-bytes (Goes to max AVX512)
  //
  // Count is taken as number of entities (vcopy implicitly assumes entity = bytes)
  //
  // ## Src Flags are as follows (2x4-bit numbers):
  //  [Src1] [Target1]
  //
  // ## Src1, Target1 has the following value composition
  // - 1-7: Register r2 through r7 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // the 32-bit in base src1, base target2 gets treated as +-offset
  01 => vcopy,
  // Move data between registers
  //
  // The MOV method has a few powerful features:
  // - if source register id == target register id, the src register id is written with its own POINTER
  // - if source register id == target register id == 12 (which is an invalid register id), the register r1 is written with the pointer to current scratchpad
  // - if source register id == target register id == 13 (which is an invalid register id), the register r1 is written with the pointer to current largepad
  //
  // `mov <source register id (4bits)> <target register id (4bits)>`
  02 => mov,
  // Load a value in a Register.
  // This is a scalar instruction that writes a constant to the register
  //
  // `reg <register (8bits)> 0xFFFFFFFFFFFFFFFF (64-bits)`
  //
  // Please note that you must ensure the data follows
  // Little-Endian Standard
  03 => reg,
  // This is bytecode resolver guidance
  // The mark is followed by a 64-bit id
  //
  // It is assumed Little-Endian but has no significant interpretation
  //
  // `mark 0xFFFFFFFFFFFFFFFF`
  //
  // This address will be used by `async`, `jz`, `jnz`, `jmp`
  04 => mark,

  // Control Flow Arguments
  //
  // These take a 64 bit jump target
  //
  // `jmp 0xFFFFFFFFFFFFFFFF`
  // `jz 0xFFFFFFFFFFFFFFFF`
  // `jnz 0xFFFFFFFFFFFFFFFF`
  05 => jmp,
  // Jump-IF
  //
  // `jif <intent id (1bit)> <width (2-bits)> <padding (1-bit)> <location src (4-bits)> <offset as i32> <marker (64-bit)>`
  //
  // ## Location Src has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // ## Widths
  // 0 : Read as u64
  // 1 : Read as u32
  // 2 : Read as u16
  // 3 : Read as u8
  //
  // offset is in terms of ENTITIES
  //
  // | Intent ID                | Description                |
  // | ------------------------ | -------------------------- |
  // | 0                        | JZ, Jump if Zero           |
  // | 1                        | JNZ, Jump if Not-Zero      |
  06 => jif,
  // This is a multi-purpose SIMD Acceleratable, scalar deoptimizable parallel
  // CMP Operation
  //
  // ## Syntax
  // `vcmp <count tag (1-bit)> <width tag (2-bits)> <operation (5-bit)> <src flags as u16> <count in u32> <base src1 as i32> <base src2 as i32> <base target3 as i32>`
  //
  // # Count tag
  // - 0: Treat the next as absolute
  // - 1: Get count from r1, the next 32-bits treated as expected count (optimization hint)
  //
  // # Width Tag
  // - 00: 8-bit
  // - 01: 16-bit
  // - 10: 32-bit
  // - 11: 64-bit
  //
  // | Operation                | Description                   |
  // | ------------------------ | ----------------------------- |
  // |    INTEGRAL OPS          |                               |
  // | 0                        | Equal                         |
  // | 1                        | Not Equal                     |
  // | 2                        | Signed Less Than              |
  // | 3                        | Unsigned Less Than            |
  // | 4                        | Signed Less Than Equal        |
  // | 5                        | Unsigned Less Than Equal      |
  // | 6                        | Signed Greater Than           |
  // | 7                        | Unsigned Greater Than         |
  // | 8                        | Signed Greater Than Equal     |
  // | 9                        | Unsigned Greater Than Equal   |
  // |    FLOATING OPS          |                               |
  // | 10                       | Ordered                       |
  // | 11                       | Unordered                     |
  // | 12                       | Equal                         |
  // | 13                       | NotEqual                      |
  // | 14                       | OrderedNotEqual               |
  // | 15                       | UnorderedOrEqual              |
  // | 16                       | LessThan                      |
  // | 17                       | LessThanOrEqual               |
  // | 18                       | GreaterThan                   |
  // | 19                       | GreaterThanOrEqual            |
  // | 20                       | UnorderedOrLessThan           |
  // | 21                       | UnorderedOrLessThanOrEqual    |
  // | 22                       | UnorderedOrGreaterThan        |
  // | 23                       | UnorderedOrGreaterThanOrEqual |
  //
  // Count is taken as Count and not bytes
  //
  // ## Src Flags are as follows (4x4-bit numbers):
  //  [Src1] [Src2] [Target1] [PADDING]
  //
  // ## Src1, Src2, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // Please note the the target1 is treated as a scalar or vector output following this rubric (from cranelift docs):
  // - When comparing scalars, the result is: - 1 if the condition holds. - 0 if the condition does not hold.
  // - When comparing vectors, the result is: - -1 (i.e. all ones) in each lane where the condition holds. - 0 in each lane where the condition does not hold.
  //
  // the next 32-bit gets treated as +-offset
  07 => vcmp,

  // --- Arithmatic (+Vectored)

  // True Vectored Addition Operator
  //
  // It is automatically deoptimized to the
  // highest SIMD level supported
  //
  // ## Syntax
  // `vadd <flags as u32 [4 bytes]> <count in u32> <base src1 as i32> <base src2 as i32> <base target1 as i32>`
  //
  // Flags are like this:
  //   [<type tag (3 bits)> <count bit>] [Src1 (4-bits)] [Src2 (4-bits)] [Target1 (4-bits)] [<Carry/Sigflow bit>] [<saturation bit>] [<aligned bit>] [Padding]
  //
  // # Carry/Sigflow bit
  // - 0: Does not emit carry or other flags
  // - 1: Transforms into ADC, r5 is treated as carry bit and it sets overflow in r5, please note this needs count=1 exactly
  //
  // # Aligned BIT
  // If set to `1` it signals the JIT compiler to assume alignment. `UNSAFE`: If unsure, it can lead to Heisenbergs that we currently don't check at the interpreter
  // stage. So set, only if sure
  //
  // # If Saturation bit is `1`, we use saturating ADD. Example below:
  // i8: 120 + 30 = 127
  // u8: 250 + 30 = 255
  //
  // (We automatically handle SIMD vs NON-SIMD saturating issues)
  //
  // # Count bit
  // - 0: Treat the next as absolute
  // - 1: Get count from r1, the u32 count is treated as expected count (optimization hint)
  //
  // ## Src1, Src2, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // the next 32-bit gets treated as +-offset
  08 => vadd,
  // True Vectored Floating Addition Operator
  //
  // It is automatically deoptimized to the
  // highest SIMD level supported
  //
  // ## Syntax
  // `vaddf <flags as u16> <count in u32> <base src1 as i32> <base src2 as i32> <base target1 as i32>`
  //
  // The carry is stored exactly how `cmp` stores it, you can jif for overflow (and select your type, unsigned or unsigned) to get the carry bit
  //
  // # Type tag is defined above
  // The flags is split like this into (4-bits + 3 x 4-bit parts):
  //   [0 <inst defined> <float type> <count bit>] [Src1] [Src2] [Target1]
  //
  // <float type>: 0 = f64, 1 = f32
  // <inst defined> = No definition
  //
  // # Count bit
  // - 0: Treat the next as absolute
  // - 1: Get count from r1, the u32 count is treated as expected count (optimization hint)
  //
  // ## Src1, Src2, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // the next 32-bit gets treated as +-offset
  09 => vaddf,
  // True Vectored Subtraction Operator
  //
  // It is automatically deoptimized to the
  // highest SIMD level supported
  //
  // ## Syntax
  // `vsub <flags as u32 [4 bytes]> <padding [4 bytes]> <count in u32> <base src1 as i32> <base src2 as i32> <base target1 as i32>`
  //
  // The layout is similar and idompotent to `vadd` with the below difference
  //
  // # Carry/Sigflow bit
  // - 0: Does not emit carry or other flags
  // - 1: Transforms into SBB, r5 is treated as borrow bit and it sets flags like borrow in r5 back, please note this needs count=1 exactly
  10 => vsub,
  // True Vectored Floating Subtraction Operator
  //
  // It is automatically deoptimized to the
  // highest SIMD level supported
  //
  // ## Syntax
  // `vsubf <flags as u16> <count in u32> <base src1 as i32> <base src2 as i32> <base target1 as i32>`
  //
  // The layout is similar and idompotent to `vaddf` with the below difference
  //
  // <inst defined>: None
  11 => vsubf,
  // True Vectored Multiplication Operator
  //
  // It is automatically deoptimized to the
  // highest SIMD level supported
  //
  // ## Syntax
  // `vmul <flags as u32 [4 bytes]> <padding [4 bytes]> <count in u32> <base src1 as i32> <base src2 as i32> <base target1 as i32>`
  //
  // The layout is similar and idompotent to `vadd` with the below difference
  //
  // Flags are like this:
  //   [<type tag (3 bits)> <count bit>] [Src1 (4-bits)] [Src2 (4-bits)] [Target1 (4-bits)] [<Extended Flags (2 bits)>] [Padding]
  //
  // The extended flags:
  // - x0: Output the 1st 32-bits (i.e. low bits)
  // - x1: Output the 2nd 32-bit (i.e. high bits)
  // - 1x: we use Wide Multiplication (target must be able to store upto 2x the count)
  // - 0x: we use Lossy Multiplication (this is only time the other bit is read)
  12 => vmul,
  // True Vectored Floating Subtraction Operator
  //
  // It is automatically deoptimized to the
  // highest SIMD level supported
  //
  // ## Syntax
  // `vmulf <flags as u16> <count in u32> <base src1 as i32> <base src2 as i32> <base target1 as i32>`
  //
  // The layout is similar and idompotent to `vaddf` with the below difference
  13 => vmulf,
  // Integer Division Operator
  //
  // ## Syntax
  // `div <args as u16> <base src1 as i32> <base src2 as i32> <base target1 as i32>`
  //
  // The 16-bit args are distributed as follows (4x4-bit slices):
  //   [Type tag] [Src1] [Src2] [Target1]
  //
  // Standard integer division emits no flags
  //
  // ## Src1, Src2, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // the next 32-bit gets treated as +-offset
  14 => div,
  // Integer Remainder Operator
  //
  // ## Syntax
  // `rem <args as u16> <base src1 as i32> <base src2 as i32> <base target1 as i32>`
  //
  // The 16-bit args are distributed as follows (4x4-bit slices):
  //   [Type tag] [Src1] [Src2] [Target1]
  //
  // ## Src1, Src2, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // the next 32-bit gets treated as +-offset
  15 => rem,
  // True Vectored Floating Subtraction Operator
  //
  // It is automatically deoptimized to the
  // highest SIMD level supported
  //
  // ## Syntax
  // `vdivf <flags as u16> <count in u32> <base src1 as i32> <base src2 as i32> <base target1 as i32>`
  //
  // The layout is similar and idompotent to `vaddf` with the below difference
  //
  // # Carry/Sigflow bit: Has no significance
  16 => vdivf,
  // Casting types operation
  //
  // `cast <flags as u16> <base src1 as i32> <base target1 as i32>`
  //
  // The 16-bit args are distributed as follows (4x4-bit slices):
  //   [Type tag Initial] [Type tag Final] [Src1] [Target1]
  //
  // This only supports:
  // - sextend
  // - uextend
  // - ireduce
  // - fdemote
  // - fpromote
  //
  // ## Src1, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  17 => cast,

  // True Vectored Negation Operator
  //
  // It is automatically deoptimized to the
  // highest SIMD level supported
  //
  // ## Syntax
  // `vneg <flags as u16 [2 bytes]> <count in u32> <base src1 as i32> <base target1 as i32>`
  //
  // Flags are like this:
  //   <type tag (4 bits)> [Src1 (4-bits)] [Target1 (4-bits)] <count bit> [Padding (3bits)]
  //
  // Please note that this is defined only for `i*` types and `f*` types. neg of iN::MIN is undefined
  //
  // # Count bit
  // - 0: Treat the next as absolute
  // - 1: Get count from r1, the u32 count is treated as expected count (optimization hint)
  //
  // ## Src1, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // the next 32-bit gets treated as +-offset
  18 => vneg,
  // Similar to `vneg` in terms of syntax.
  // Does the abs(x) operation
  //
  // Only defined for `i*` and `f*` types, abs(iN::MIN) is not defined
  19 => vabs,

  // Vectored Floating Operation
  //
  // It is automatically deoptimized to the
  // highest SIMD level supported
  //
  // ## Syntax
  // `vfop <flags as u16 [2 bytes]> <count in u32> <base src1 as i32> <base target1 as i32>`
  //
  // Flags are like this:
  //   [padding (3-bits)] [float type (1 bit)] [Src1 (4-bits)] [Target1 (4-bits)] [count bit (1-bit)] [Sub-Op (3-bit)]
  //
  // ## Float Type
  // - 0: f64
  // - 1: f32
  //
  // Please note that this is defined only for `f*` types.
  //
  // ## Sub-Op
  // - 0: ceil
  // - 1: floor
  // - 2: trunc
  // - 3: nearest
  // - 4: sqrt
  //
  // # Count bit
  // - 0: Treat the next as absolute
  // - 1: Get count from r1, the u32 count is treated as expected count (optimization hint)
  //
  // ## Src1, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // the next 32-bit gets treated as +-offset
  20 => vfop,

  // Vectored Floating Cast
  //
  // It is automatically deoptimized to the
  // highest SIMD level supported
  //
  // ## Syntax
  // `vfcast <flags as u16 [2 bytes]> <count in u32> <base src1 as i32> <base target1 as i32>`
  //
  // Flags are like this:
  //   [Padding] [count bit (1-bit)] [op (1-bit)] [f width (1-bit)] [int type tag (3 bits)] [Src1 (4-bits)] [Target1 (4-bits)]
  //
  // f width: 0 -> f64, 1 -> f32
  //
  // int type tag: well, obvious
  //
  // # Count bit
  // - 0: Treat the next as absolute
  // - 1: Get count from r1, the u32 count is treated as expected count (optimization hint)
  //
  // # Op Bit
  // - 0: Convert the float in src1 into the integer type specific in target1
  // - 1: Convert the integer in src1 into float of target1
  //
  // ## Src1, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // the next 32-bit gets treated as +-offset
  21 => vfcast,

  // The below bitwise operation has the exact true verctored syntax like vabs with no undefined behaviour
  //
  // It is automatically deoptimized to the
  // highest SIMD level supported
  //
  // ## Syntax
  // `vb* <flags as u16> <Op (4-bits)> <padding (3-bits)> <count bit (1-bit)> <count in u32> <base src1 as i32> <base src2 as i32> <base target1 as i32>`
  //
  // The carry is stored exactly how `cmp` stores it, you can jif for overflow (and select your type, unsigned or unsigned) to get the carry bit
  //
  // # Type tag is defined above
  // The flags is split like this into (4-bits + 3 x 4-bit parts):
  //   [Width (2-bits)] [Padding (2-bits)] [Src1] [Src2] [Target1]
  //
  // # Width bits
  // - 00: u64
  // - 01: u32
  // - 10: u16
  // - 11: u8
  //
  // # Op
  // - 0: and (x & y)
  // - 1: or (x | y)
  // - 2: xor (x ^ y)
  // - 3: not (~x) (src2 is ignored)
  // - 4: or_not (x | ~y)
  // - 5: and_not (x & ~y)
  // - 6: xor_not (x ^ ~y)
  // - 7: bitrev (src2 is ignored) [SCALAR ONLY; LOOP EMITTED FOR COUNT > 1]
  // - 8: bswap (src2 is ignored) [SCALAR ONLY; LOOP EMITTED FOR COUNT > 1]
  //
  // # src2 rules
  // To protect the integrity, for situations where src2 is ignored, it (spoiler!) it is
  // indeed READ OUT! Hence, set src2=src1
  //
  // # Count bit
  // - 0: Treat the next as absolute
  // - 1: Get count from r1, the u32 count is treated as expected count (optimization hint)
  //
  // ## Src1, Src2, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // the next 32-bit gets treated as +-offset
  22 => vbit,

  // Truly Vectored Rotation
  //
  // These are SIMD Acceleratable
  // ## Syntax
  // `vrot <flags as u16> <padding (6-bits)> <rotation bit (1-bit)> <count bit (1-bit)> <count in u32> <base src1 as i32> <amount src i.e. src2 as i32> <base target1 as i32>`
  //
  // # Type tag is defined above
  // The flags is split like this into (4-bits + 3 x 4-bit parts):
  //   [Type Tag (4-bits)] [Src1] [Src2] [Target1]
  //
  // Src2 i.e. amount to shift by is a scalar not a vector!!! Also, it is to be the UNSIGNED variant of the bit.
  // Src2 defines how many bits to shift
  // Example:
  // - for `shl` of i64 types, you need to pass Src2 as 1 single u64 no matter the count
  // - for `shl` of u64 types, you need to pass Src2 as 1 single u64 no matter the count
  //
  // All the lanes are equally bit-shifted
  //
  // # Rotation bit
  // - 0: rotl (Rotate Left)
  // - 1: rotr (Rotate Right)
  //
  // ## Src1, Src2, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // the next 32-bit gets treated as +-offset
  23 => vrot,

  // Truly Vectored SHL & SHR
  //
  // These are SIMD Acceleratable
  // ## Syntax
  // `vsh <flags as u16> <padding (6-bits)> <op bit (1-bit)> <count bit (1-bit)> <count in u32> <base src1 as i32> <amount i.e. src2 as i32> <base target1 as i32>`
  //
  // # Type tag is defined above
  // The flags is split like this into (4-bits + 3 x 4-bit parts):
  //   [Type Tag (4-bits)] [Src1] [Src2] [Target1]
  //
  // Floating types are not supported!
  // Src2 i.e. amount to shift by is a scalar not a vector!!! Also, it is to be the UNSIGNED variant of the bit.
  // Example:
  // - for `shl` of i64 types, you need to pass Src2 as 1 single u64 no matter the count
  // - for `shl` of u64 types, you need to pass Src2 as 1 single u64 no matter the count
  //
  // All the lanes are equally bit-shifted
  //
  // # Count bit
  // - 0: Treat the next as absolute
  // - 1: Get count from r1, the u32 count is treated as expected count (optimization hint)
  //
  // # Op bit
  // - 0: SHL
  // - 1: SHR
  //
  // ## Src1, Src2, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // the next 32-bit gets treated as +-offset

  24 => vsh,

  // True Vectored Count
  //
  // Only POPCNT is converted to SIMD variants
  // Others are converted to a scalar loop
  //
  // ## Syntax
  // `vcnt <flags as u16 [2 bytes]> <count in u32> <base src1 as i32> <base target1 as i32>`
  //
  // Flags are like this:
  //   [<width (2 bits)> <count bit>] [Src1 (4-bits)] [Target1 (4-bits)] [Op (4-bits)]
  //
  // # Width
  // - 0: 64-bits
  // - 1: 32-bits
  // - 2: 16-bits
  // - 3: 8-bits
  //
  // # Count bit
  // - 0: Treat the next as absolute
  // - 1: Get count from r1, the u32 count is treated as expected count (optimization hint)
  //
  // # Op
  // - 0: POPCNT (Population Count)
  // - 1: Count leading zeroes
  // - 2: Count leading sign bits
  // - 3: Count trailing zeroes
  //
  // ## Src1, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // the next 32-bit gets treated as +-offset
  25 => vcnt,
  // Truly Vectored Min-Max Operator
  //
  // These are SIMD Acceleratable
  // ## Syntax
  // `vminimax <flags as u16> <padding (5-bits)> <aligned (1-bit)> <count bit (1-bit)> <Max (1-bit)> <count in u32> <base src1 as i32> <base src2 as i32> <base target1 as i32>`
  //
  // # Type tag is defined above
  // The flags is split like this into (4-bits + 3 x 4-bit parts):
  //   [Type Tag (4-bits)] [Src1] [Src2] [Target1]
  //
  // # Max bit
  // - 0: min (Minimum)
  // - 1: max (Maximum)
  //
  // ## Src1, Src2, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // the next 32-bit gets treated as +-offset
  26 => vminimax,

  // True Vectored Floating Fused-Multiply-Add Operator
  //
  // It is automatically deoptimized to the
  // highest SIMD level supported
  //
  // ## Syntax
  // `vfma <flags as u16> <padding [6bits]> <float type> <count bit> <count in u32> <base src1 as i32> <base src2 as i32> <base src3 as i32> <base target1 as i32>`
  //
  // vfma is fused version of (src1*src2 + src3)
  //
  // # Type tag is defined above
  // The flags is split like this into (4-bits + 4 x 4-bit parts):
  //   [Src1] [Src2] [Src3] [Target1]
  //
  // <float type>: 0 = f64, 1 = f32
  //
  // # Count bit
  // - 0: Treat the next as absolute
  // - 1: Get count from r1, the u32 count is treated as expected count (optimization hint)
  //
  // ## Src1, Src2, Src3, Target1 has the following value composition
  // - 1-7: Register r2 through r8 indices
  // - 8: Small Scratchpad
  // - 9: Large Scratchpad
  // - 10: Pointer, pointer read from r2 as 64-bit pointer
  //
  // the next 32-bit gets treated as +-offset
  27 => vfma,

  // --- System and threading ---

  // Sync calling
  //
  // The syntax is
  // `synccall <section id as u64>`
  //
  // Please note that using `synccall` for async SectionID is full on undefined
  //
  // ## ⚠️ Performance Regression
  // On async module, this blocks.
  28 => synccall,
  // Async Call
  //
  // The syntax is
  // `asynccall <section id as u64> <mark id (to resume from) as u64>`
  //
  // Please note:
  // - If the calling section is async-enabled, This is perfectly cooperatively poll
  // - If the calling section is a sync section, This is perfectly block
  29 => asynccall,

  // Task and Thread Management
  //
  // `spawn <section id as u64> <flags (6-bits)> <scratchpad start index (5-bits)> <total to copy (5-bits)>`
  //
  // Note that count to copy is calculated in terms of count of 64-BIT (8 byte) chunks
  //
  // ## Flags:
  //    [TaskOut] [ASYNC] [HWND]
  // - HWND: Return a Spawn Handle (please note that failure to `task detach/join` will lead to memory leak)
  // - ASYNC: The module is an async module
  //
  // TaskOut is the location to write the handle, if HWND is selected
  //
  // ## TaskOut for ASYNC
  // For async tasks, this only writes the only TaskOut (HANDLE) [8-bytes stored]
  //
  // ## TaskOut for SYNC
  // For sync tasks, [TaskOut] stores the HANDLE and [TaskOut]+1 stores THREAD_HANDLE [16-bytes stored].
  //
  // ## Warning:
  // - Failure to correctly mark as ASYNC/SYNC can lead to undefined behaviour
  // - For SYNC Tasks, both the HANDLE and THREAD_HANDLE has to be detached/joined and detached respectively, if HWND is specified
  // - Apart from scratchpad, your current thread's FULL REGISTER MAP (r1 through r8) is copied to the new thread's memory
  30 => spawn,

  // Task operation
  //
  // `task <sub op (4-bits)> <def (4-bits)> <marker (64-bit)>`
  //
  // As marker implies, this implementation always does an implicit JMP after the task
  //
  // # Sub Op
  // - 0: async task detach
  // - 1: async task join
  // - 2: async task is_complete (always updates to r8, 0=false,!0=true)
  // - 3: sync task detach
  // - 4: sync task join
  // - 5: sync task is_complete (always updates to r8, 0=false,!0=true)
  // - 6: sync thread unpark
  // - 7: sync thread handle detach
  // - 8: sync yield (yields the current thread)
  // - 9: sync park (parks the current thread, some other thread MUST unpark it to continue)
  // - 10: async yield (yields the current task ONLY)
  // - 11: wait (ms)
  31 => task,

  // Atomic Instruction Family
  //
  // Please note the Atomics ONLY apply to pointers and numbers, hence types
  // are given as follows. Also, since these are atomics, they depend on pointers
  // hence registers cannot be atomic
  //
  // Natural alignment is forced
  //
  // [Sub Opcode (2-bits)] [type (3-bit)] [ordering (3-bits)] [offset v0 (i8)] [offset v1 (i8)] [offset v2 (i8)] [offset v3 (i8)] [instruction defined (16-bit)]
  //
  // # ordering
  // 0: SeqCst
  // 1: Relaxed
  // 2: Acquire
  // 3: Release
  // 4: Acquire-Release
  //
  // Please Note : Only our interpreter and LLVM Compiler JIT respects the ordering, cranelift is pinned by `SeqCst`. It should not
  // lead to real world instability as the interpreter still respects ordering to ensure races don't show up ONLY in LLVM JIT
  //
  // ## Sub Opcode
  // 00: CAS
  // 01: LOAD
  // 10: RMW
  // 11: STORE
  //
  // # Type
  // <follows standard protocol>
  //
  // # CAS
  // CAS stands for `Compare-And-Swap`
  // Here is the instruction defined space
  // ret[4-bit] e[4-bit] x[4-bit] p1[4-bit]
  //
  // ret (v3) = Return location [Allocate 2x the type width; 1st stores the fetched output, 2nd stores a boolean (all ones for true, zeroes for false)]
  // e (v2) = Expected value (in the expected type)
  // x (v1) = Value to be stored (in the type)
  // p1 (v0) = Location that contains pointer (read as usize, assume u64 for cross-compatibility)
  //
  // Atomic Ordering specified above is for SUCCESS case
  // For FAILURE, we look at r8 for atomic ordering, so please ensure it is correct [read as u8 i.e. bits 0to7]
  //
  // # Store
  // Stores a value to the atomic memory region
  //
  // Instruction defined space
  // [Padding (8-bits)] x[4-bit] p1[4-bit]
  //
  // p1 (v0) = Location that contains pointer (read as u64, assume u64 for cross-compatibility)
  // x (v1) = Location that defines the value to be stored
  //
  // # RMW
  // Atomically read-modify-write
  //
  // op[4-bit] o[4-bit] ret[4-bit] p1[4-bit]
  //
  // v3 is IGNORED
  // o (v2) = Location to the operand
  // ret (v1) = Location to where the value be returned (ret==o is legal)
  // p1 (v0) = Location that contains pointer (read as u64, assume u64 for cross-compatibility)
  //
  // # Load
  //
  // Atomically load memory from p1 address
  //
  // [Padding] ret[4-bit] p1[4-bit]
  //
  // p1 (v0) = Location that contains pointer (read as u64, assume u64 for cross-compatibility)
  // ret (v1) = Location to where to store
  32 => atomic,

  // Scratchpad Management Protocols
  //
  // `scratch class[2-bit] [defined (14-bits)]`
  //
  // If class is 00, it means to allocate
  // If class is 01, it means to dealloc
  // If class is 10, it means to dealloc (but the allocate request had alignment set)
  //
  // # Allocate
  // This defines two more fields
  // `[padding (6-bits)] size_reg[4-bits] align_reg[4-bits]`
  //
  // Note: Size must be as `u64` and size_reg follows the standard register numbers (defined at top)
  // Align is also fetched from register mentioned in 4-bits
  //
  // - Failing to dealloc this allocated chunk is considered undefined behaviour.
  // - Allocating another chunk is also considered undefined behaviour.
  //
  // # Reallocate (Future Spec)
  //
  // `padding[2-bits] old_size_reg[4-bits] new_size_reg[4-bits] align_reg[4-bits]`
  //
  // Its similar to allocate
  //
  // # Dealloc
  //
  // This takes no extra arguments!
  33 => scratch
}
