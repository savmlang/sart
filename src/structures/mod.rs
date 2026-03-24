use std::{os::raw::c_void, ptr::null};

pub mod ffi;

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

  // SIMD
  pub u8x8: [u8; 8],
  pub u16x4: [u16; 4],
  pub u32x2: [u32; 2],
  pub i8x8: [i8; 8],
  pub i16x4: [i16; 4],
  pub i32x2: [i32; 2],
  pub f32x2: [f32; 2],

  // Complex
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
