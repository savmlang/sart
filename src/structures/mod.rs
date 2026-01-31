use saffi::{boxed::RTSafeBoxWrapper, ctr::Instruction};

use dashmap::DashMap;

use std::{
  os::raw::c_void,
  ptr::null,
  sync::{Arc, LazyLock},
};

#[repr(C)]
#[derive(Clone, Copy)]
pub union QuadPackedData {
  pub u64: u64,
  pub i64: i64,
  pub u32: u32,
  pub i32: i32,
  pub u16: u16,
  pub i16: i16,
  pub u8: u8,
  pub i8: i8,
  pub f32: f32,
  pub f64: f64,
  pub complex: *mut RTSafeBoxWrapper,
  pub pointer: *mut c_void,
  pub selfref: *mut Self,

  #[doc(hidden)]
  pub _checknull: *const c_void,
}

impl QuadPackedData {
  #[inline(always)]
  pub fn nullify(&mut self) {
    self._checknull = null();
  }

  #[inline(always)]
  pub fn heap(&mut self) -> &mut Self {
    self
  }
}

pub struct EnforceNoCopy;

/// This `u64` is a packed data
/// 1st 32=bits (i.e. u32) is module id
/// 2nd 32=bit (i.e. u32) is module section
pub type CompiledCode = Arc<
  DashMap<
    u64,
    LazyLock<Box<[Instruction]>, Box<dyn FnOnce() -> Box<[Instruction]>>>,
    ahash::RandomState,
  >,
>;
