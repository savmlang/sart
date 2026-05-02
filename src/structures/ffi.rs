use std::{
  collections::HashMap,
  ffi::c_void,
  iter,
  marker::PhantomPinned,
  mem::zeroed,
  num::NonZero,
  ptr::{addr_of_mut, null_mut},
};

use libffi_sys::{
  FFI_TYPE_STRUCT, ffi_type, ffi_type_double, ffi_type_float, ffi_type_sint8, ffi_type_sint16,
  ffi_type_sint32, ffi_type_sint64, ffi_type_uint8, ffi_type_uint16, ffi_type_uint32,
  ffi_type_uint64,
};
use serde::{Deserialize, Serialize};

// (section id) -> FnDecl
pub type LibraryResolverStructure = HashMap<u64, FDecl>;

pub use libffi_sys;

#[derive(Debug, Serialize, Deserialize)]
pub struct FDecl {
  pub symbol: Box<[u8]>,
  pub sig: CallSig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum CallSig {
  /// Define your own Calling Signature
  CDef(CDef),

  /// Uses the SaFFI Calling signature
  ///
  /// it is defined always as
  /// ```rust
  ///   extern "C" (*mut sart::ctr::CVMTaskState) -> ()
  /// ```
  SaFFI(
    /// Helps JIT do speculative cleaning
    UnsafeSaFFIProfile,
  ),
  /// Uses the SaFFI Async Calling signature
  ///
  /// ## Important
  /// The Future Task is supposed to NOT Depend on the pointer to CVMTaskState
  /// The function is allowed to modify registers until it sends a FutureTask
  ///
  /// it is defined always as
  /// ```rust
  ///   extern "C" (*mut sart::ctr::CVMTaskState, *mut saffi::futures::FutureTask<u64>) -> ()
  /// ```
  ///
  /// ## STANDARD SaFFI Out Conv
  /// The low 64-bit bits is copied to r7
  ///
  /// ##
  SaFFIAsyncQ(
    /// Helps JIT do speculative cleaning
    UnsafeSaFFIProfile,
  ),
  /// Uses the SaFFI Async Calling signature
  ///
  /// ## Important
  /// The Future Task is supposed to NOT Depend on the pointer to CVMTaskState
  /// The function is allowed to modify registers until it sends a FutureTask
  ///
  /// it is defined always as
  /// ```rust
  ///   extern "C" (*mut sart::ctr::CVMTaskState, *mut saffi::futures::FutureTask<[u64; 2]>) -> ()
  /// ```
  ///
  /// ## STANDARD SaFFI Out Conv
  /// The low 64-bits is copied to r7
  /// The high 64-bits is copied to r8
  SaFFIAsyncO(
    /// Helps JIT do speculative cleaning
    UnsafeSaFFIProfile,
  ),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UnsafeSaFFIProfile {
  // bit 0..8 = R1..R8
  regclobber: u8,
}

impl UnsafeSaFFIProfile {
  pub fn default() -> Self {
    Self { regclobber: 0xFF }
  }

  pub fn clobbers(&self, reg: VReg) -> Result<bool, ParseError> {
    Ok(self.regclobber & Self::rmask(&reg)? != 0)
  }

  fn rmask(rg: &VReg) -> Result<u8, ParseError> {
    Ok(match rg {
      VReg::R1 => 0x01,
      VReg::R2 => 0x02,
      VReg::R3 => 0x04,
      VReg::R4 => 0x08,
      VReg::R5 => 0x10,
      VReg::R6 => 0x20,
      VReg::R7 => 0x40,
      VReg::R8 => 0x80,
      _ => return Err(ParseError::FoundInvalidReg),
    })
  }

  /// # Safety
  /// The caller must ensure that `regsclobber` accurately represent
  /// the behavior of the external function. Providing an incomplete list may
  /// lead to the JIT/VM making incorrect assumptions about register state,
  /// resulting in Undefined Behavior.
  pub unsafe fn new(regsclobber: &[VReg]) -> Result<Self, ParseError> {
    let mut out = Self { regclobber: 0 };

    let o = &mut out.regclobber;
    for u in regsclobber {
      *o |= Self::rmask(u)?;
    }

    Ok(out)
  }
}

#[derive(Debug)]
pub enum ParseError {
  FoundInvalidReg,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CDef {
  pub inargs: Box<[MapValue]>,
  pub out: COut,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum COut {
  /// ```rust
  ///   extern "C" fn(...) -> ()
  /// ```
  Void,
  /// ```rust
  ///   extern "C" fn(...) -> u8
  /// ```
  ///
  /// ## Implementation semantics
  /// The data is r7 is to be assumed to be fully clobbered
  ///
  /// The 8-bits is populated in r7.u8.offset(0)
  ///
  /// Follows `STANDARD SaFFI Out Conv`
  Bits8,
  /// ```rust
  ///   extern "C" fn(...) -> u16
  /// ```
  ///
  /// ## Implementation semantics
  /// The data is r7 is to be assumed to be fully clobbered
  ///
  /// The 16-bits is populated in r7.u16.offset(0)
  ///
  /// Follows `STANDARD SaFFI Out Conv`
  Bits16,
  /// ```rust
  ///   extern "C" fn(...) -> u32
  /// ```
  ///
  /// The 32-bits is populated in r7.u32.offset(0)
  ///
  /// ## Implementation semantics
  /// The data is r7 is to be assumed to be fully clobbered
  ///
  /// Follows `STANDARD SaFFI Out Conv`
  Bits32,
  /// ```rust
  ///   extern "C" fn(...) -> u64
  /// ```
  ///
  /// The low 64-bits is populated in r7
  ///
  /// Follows `STANDARD SaFFI Out Conv`
  Bits64,
  /// ```rust
  ///   extern "C" fn(...) -> [u64; 2]
  /// ```
  ///
  /// The low 64-bits is populated in r7
  /// The high 64-bits is populated in r8
  ///
  /// Follows `STANDARD SaFFI Out Conv`
  Bits128,
}

impl COut {
  pub fn width(&self) -> usize {
    match self {
      Self::Void => 0,
      COut::Bits8 => 1,
      COut::Bits16 => 2,
      COut::Bits32 => 4,
      COut::Bits64 => 8,
      COut::Bits128 => 16,
    }
  }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct MapValue {
  pub vtype: VType,
  pub vreg: VReg,
  /// Offset in terms of `count`
  ///
  /// Exception : bytes, it is interpreted as bytes then
  pub regof: u8,
}

impl VReg {
  pub fn as_locsrc(&self) -> u8 {
    match self {
      Self::R1 => 0,
      Self::R2 => 1,
      Self::R3 => 2,
      Self::R4 => 3,

      Self::R5 => 4,
      Self::R6 => 5,
      Self::R7 => 6,
      Self::R8 => 7,

      Self::Scratchpad => 8,
      Self::Largepad => 9,
      Self::LoadFromPtrInR2 => 10,
    }
  }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum VType {
  U64,
  U32,
  U16,
  U8,
  /// `usize`
  ///
  /// SaVM has no concept of `usize` and hence it reads the value as
  /// `u64` and shrinks to `usize` by only reading low bits
  USize,
  I64,
  I32,
  I16,
  I8,
  /// `isize`
  ///
  /// SaVM has no concept of `isize` and hence it reads the value as
  /// `i64` and shrinks to `isize` by only reading low bits
  ISize,
  F32,
  F64,

  /// Read the first `n` bytes
  Bytes(U3),
}

pub struct LFFITypeMap {
  _deps: [*mut ffi_type; 16],
  pub lffitype: ffi_type,
  _pin: PhantomPinned,
}

impl From<ffi_type> for LFFITypeMap {
  fn from(value: ffi_type) -> Self {
    Self {
      _deps: unsafe { zeroed() },
      lffitype: value,
      _pin: PhantomPinned,
    }
  }
}

impl VType {
  pub fn width(&self) -> u32 {
    match self {
      Self::U8 => 1,
      Self::U16 => 2,
      Self::U32 => 4,
      Self::U64 => 8,
      Self::USize => (|| {
        #[cfg(target_pointer_width = "32")]
        return 4;

        #[cfg(target_pointer_width = "64")]
        return 8;
      })(),

      // Signed
      Self::I8 => 1,
      Self::I16 => 2,
      Self::I32 => 4,
      Self::I64 => 8,
      Self::ISize => (|| {
        #[cfg(target_pointer_width = "32")]
        return 4;

        #[cfg(target_pointer_width = "64")]
        return 8;
      })(),

      Self::F32 => 4,
      Self::F64 => 8,

      Self::Bytes(n) => n.get() as u32,
    }
  }

  pub fn as_savmtype(&self) -> u8 {
    match self {
      Self::U8 => 3,
      Self::U16 => 2,
      Self::U32 => 1,
      Self::U64 => 0,
      Self::USize => (|| {
        #[cfg(target_pointer_width = "32")]
        return 1;

        #[cfg(target_pointer_width = "64")]
        return 0;
      })(),

      // Signed
      Self::I8 => 7,
      Self::I16 => 6,
      Self::I32 => 5,
      Self::I64 => 4,
      Self::ISize => (|| {
        #[cfg(target_pointer_width = "32")]
        return 5;

        #[cfg(target_pointer_width = "64")]
        return 4;
      })(),

      Self::F32 => 9,
      Self::F64 => 8,

      Self::Bytes(_) => u8::MAX,
    }
  }

  pub fn ptr(&self, pt: *mut c_void, regof: u8) -> *mut c_void {
    unsafe {
      let offset = self.width() * (regof as u32);

      (pt as *mut u8).byte_add(offset as usize) as _
    }
  }

  pub unsafe fn as_lffitype(&self, slot: &mut LFFITypeMap) {
    slot.lffitype = unsafe {
      match self {
        Self::U8 => ffi_type_uint8,
        Self::U16 => ffi_type_uint16,
        Self::U32 => ffi_type_uint32,
        Self::U64 => ffi_type_uint64,
        Self::USize => (|| {
          #[cfg(target_pointer_width = "32")]
          return ffi_type_uint32;

          #[cfg(target_pointer_width = "64")]
          return ffi_type_uint64;
        })(),

        // Signed
        Self::I8 => ffi_type_sint8,
        Self::I16 => ffi_type_sint16,
        Self::I32 => ffi_type_sint32,
        Self::I64 => ffi_type_sint64,
        Self::ISize => (|| {
          #[cfg(target_pointer_width = "32")]
          return ffi_type_sint32;

          #[cfg(target_pointer_width = "64")]
          return ffi_type_sint64;
        })(),

        Self::F32 => ffi_type_float,
        Self::F64 => ffi_type_double,

        Self::Bytes(n) => {
          assert!(n.get() < 16, "Bytes too large for _deps");
          (0..(n.get()))
            .map(|_| addr_of_mut!(ffi_type_uint8))
            .chain(iter::once(null_mut()))
            .zip(slot._deps.iter_mut())
            .for_each(|(n, t)| {
              *t = n;
            });

          slot.lffitype = ffi_type {
            type_: FFI_TYPE_STRUCT,
            elements: slot._deps.as_mut_ptr(),
            ..Default::default()
          };

          return;
        }
      }
    };
  }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
/// A structure, that stores from 1..=8
pub struct U3(NonZero<u8>);

impl U3 {
  pub const fn new(data: u8) -> Self {
    assert!(1 <= data && data <= 8);

    Self(unsafe { NonZero::new_unchecked(data) })
  }

  pub const fn get(self) -> u8 {
    self.0.get()
  }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum VReg {
  R1,
  R2,
  R3,
  R4,
  R5,
  R6,
  R7,
  R8,
  Scratchpad,
  Largepad,
  LoadFromPtrInR2,
}
