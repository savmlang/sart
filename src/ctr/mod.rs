use core::ffi::c_void;

use crate::structures::QuadPackedData;

/// the first argument actually is a reference to the VM Struct lol
pub type DispatchFn = extern "C" fn(*mut c_void, *mut VMTaskState, u64);

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
  pub module: u64,
  pub dispatch: DispatchFn,
}

/// This is the program registry state
/// This is created for each thread executed
/// under the Runtime
///
/// PLEASE DO NOT CHANGE THE LAYOUT, IT MAY BREAK
// 32-bit CPUs benefit from 64-bit alignment as well, not changing alignment
#[repr(C, align(64))]
pub struct VMTaskState {
  /// Registers : `r1 through r6`
  /// Please note the this means that the data is simply a 64-bit value with intension defined by the instruction
  pub r1: QuadPackedData,
  pub r2: QuadPackedData,
  pub r3: QuadPackedData,
  pub r4: QuadPackedData,
  pub r5: QuadPackedData,
  pub r6: QuadPackedData,
  /// If this is NULL, this means this itself is the super task
  pub super_: *mut VMTaskState,
  /// Instruction pointer, but represented as bytecode terminology
  pub curline: usize,
}

const _CHECK_REGISTER_SET_SIZE: usize =
  size_of::<VMTaskState>() - 1 * size_of::<usize>() - size_of::<*mut VMTaskState>();
pub const REGISTER_SET_SIZE: usize = 6 * size_of::<QuadPackedData>();

const _OUT: bool = REGISTER_SET_SIZE == _CHECK_REGISTER_SET_SIZE;

const _ONE_CPU_CACHE: bool = size_of::<VMTaskState>() == 64;
const _ENSURE_ONE_CPU_CACHE: () = assert!(_ONE_CPU_CACHE);

const _ENSURE_VMTASKSTATE_IS_VALID: () = assert!(_OUT);

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

instruction! {
    // --- I. Memory Management (Heap/Stack) ---
    // (r1, r2, r3, r4, r5, r6 typically used for operands/pointers/sizes)
    0x01 => alloc, // Allocate data from a register and move it into memory. The register should be treated as containing invalid data
    0x02 => aralc, // TODO: FUTURE SPEC
    0x03 => load,  // Load data from heap, `load 0u8 <addr>` to load from heap
    0x04 => free,  // Drop the associated complex data from the heap. Running on primitive types is undefined behaviour
    0x05 => own,   // Move value from Heap to r<n>
    0x06 => mark,  // Mark for jump instructions
    0x07 => clr,   // Clear a register/value. NOTE: This does not drop associated complex heap value
    0x08 => clrs,  // Clear all the registers/values. NOTE: This does not drop associated complex heap value

    // --- II. Basic Data & Register Operations ---
    0x09 => put_reg, // TODO: FUTURE SPEC; Put immediate value in any register. `put_reg <register u8> <total bytes u8; can it 1/2/4/8> <bytes>`

    // --- III. Control Flow ---
    0x0A => jmp,     // Unconditional jump
    0x0B => jz,      // Jump if zero
    0x0C => jnz,     // Jump if not zero
    //`ret` instruction has been removed. Please use jump calls along with super_mov to simulate the same

    0x0D => cmp,     // Compare 64-bit unsigned values (r1, r2)
    // Mov data b/w registers
    0x0E => mov,

    // --- IV. Arithmetic (r1, r2 are 64-bit unsigned inputs) ---
    0x0F => add,
    0x10 => sub,
    0x11 => mul,
    0x12 => div,
    0x13 => rem, // Remainder

    // --- V. Arithmetic Mutating (r6 is output pointer) ---
    0x14 => add_mut,
    0x15 => sub_mut,
    0x16 => mul_mut,
    0x17 => div_mut,
    0x18 => rem_mut,

    // --- VI. Bitwise (r1, r2 are 64-bit unsigned inputs) ---
    0x19 => and,
    0x1A => or,
    0x1B => xor,

    // --- VII. Bitwise Mutating (r6 is output pointer) ---
    0x1C => and_mut,
    0x1D => or_mut,
    0x1E => xor_mut,

    // --- VIII. Bitshift (r1 is value, r2 is shift amount) ---
    0x1F => shl, // Shift left
    0x20 => shr, // Shift right

    // --- IX. Bitshift Mutating (r6 is output pointer) ---
    0x21 => shl_mut,
    0x22 => shr_mut,

    // --- X. Pointer Arithmetic (r1, r2 treated as pointers and deferenced to get 64-bit unsigned memory) ---
    0x23 => add_ptr,
    0x24 => sub_ptr,
    0x25 => offset_ptr,

    // --- XII. System & Threading ---
    // TODO: Better Threading Management, eg. is_running <thread hwnd>
    0x26 => libcall,
    0x27 => spawn,   // Threading: Create new thread
    0x28 => join,    // TODO: FUTURE SPEC; Threading: Wait for thread
    0x29 => yield,   // Threading: Give up time
    0x2A => await,   // Threading: Asynchronous wait

    // --- XIII. Library-Only Instructions (Accesses Super Context) ---
    0x2B => super_mov, // Load register data from the caller. Usage `super_mov <target register> <load from register>`
    0x2C => mov_to_super
}
