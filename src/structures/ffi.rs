use std::{collections::HashMap, num::NonZero};

use serde::{Deserialize, Serialize};

// (section id) -> FnDecl
pub type LibraryResolverStructure = HashMap<u64, FDecl>;

#[derive(Debug, Serialize, Deserialize)]
pub struct FDecl {
  pub symbol: Box<[u8]>,
  pub sig: CallSig,
}

#[derive(Debug, Serialize, Deserialize)]
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
  ///   extern "C" (*mut sart::ctr::CVMTaskState) -> saffi::futures::FutureTask<u64>
  /// ```
  ///
  /// ## STANDARD SaFFI Out Conv
  /// The low 64-bit bits is copied to r7
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
  ///   extern "C" (*mut sart::ctr::CVMTaskState) -> saffi::futures::FutureTask<[u64; 2]>
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

#[derive(Debug, Serialize, Deserialize)]
pub struct UnsafeSaFFIProfile {
  // bit 0..8 = R1..R8
  regused: u8,
  regclobber: u8,
}

impl UnsafeSaFFIProfile {
  pub fn default() -> Self {
    Self {
      regclobber: 0xFF,
      regused: 0xFF,
    }
  }

  pub fn uses(&self, reg: VReg) -> Result<bool, ParseError> {
    Ok(self.regused & Self::rmask(&reg)? != 0)
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
  /// The caller must ensure that `regsused` and `regsclobber` accurately represent
  /// the behavior of the external function. Providing an incomplete list may
  /// lead to the JIT/VM making incorrect assumptions about register state,
  /// resulting in Undefined Behavior.
  pub unsafe fn new(regsused: &[VReg], regsclobber: &[VReg]) -> Result<Self, ParseError> {
    let mut out = Self {
      regclobber: 0,
      regused: 0,
    };
    for u in regsused {
      let o = &mut out.regused;

      *o |= Self::rmask(u)?;
    }

    for u in regsclobber {
      let o = &mut out.regclobber;

      *o |= Self::rmask(u)?;
    }

    Ok(out)
  }
}

#[derive(Debug)]
pub enum ParseError {
  FoundInvalidReg,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CDef {
  pub inargs: Box<[MapValue]>,
  pub out: COut,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct MapValue {
  pub vtype: VType,
  pub vreg: VReg,
  /// Offset in terms of `count`
  ///
  /// Exception : bytes, it is interpreted as bytes then
  pub regof: i8,
}

#[derive(Debug, Serialize, Deserialize)]
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
  Bytes(U3),
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
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
