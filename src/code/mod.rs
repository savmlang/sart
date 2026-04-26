use std::{
  hint::{cold_path, spin_loop},
  mem::{forget, offset_of},
  num::NonZeroU32,
  ops::Deref,
  sync::atomic::{AtomicPtr, AtomicU32, Ordering},
};

#[derive(Debug)]
pub struct SwappableCodeStore<T> {
  // [31: LOCKED] [30: PINNED] <readers count>
  lock: AtomicU32,
  code: AtomicPtr<StoredCode<T>>,
}

const LOCKED: u32 = 1 << 31;
const PINNED: u32 = 1 << 30;

const MASK: u32 = !(LOCKED | PINNED);

const SH_AMT: u32 = 30;

pub const U8_PINNED: u8 = 1 << 0;

impl<T> SwappableCodeStore<T> {
  pub fn new(code: T) -> Self {
    Self {
      lock: AtomicU32::new(0),
      code: AtomicPtr::new(Box::into_raw(Box::new(StoredCode {
        refcount: AtomicU32::new(1),
        code,
      }))),
    }
  }

  /// This also returns FLAGS, currently is the format
  ///
  /// <reserved> [0: PINNED]
  pub fn get(&self) -> (u8, CodeGuard<T>) {
    loop {
      let old = self.lock.fetch_add(1, Ordering::Acquire);

      // We were right!
      if old & LOCKED == 0 {
        let out = CodeGuard::new(self.code.load(Ordering::Acquire));

        let old = self.lock.fetch_sub(1, Ordering::Release);

        return (((old & PINNED) >> SH_AMT) as _, out);
      }

      self.lock.fetch_sub(1, Ordering::Release);

      // Fix our sins, with CAS
      loop {
        cold_path();

        let ld = self.lock.load(Ordering::Relaxed);

        if ld & LOCKED == 0 {
          break;
        }

        spin_loop();
      }
    }
  }

  /// Grabs a permanent reader and gives a fully static reference to the data
  ///
  /// ## SAFETY
  /// Please ensure to call it for less than `1073741824` (~2^30) times or else it can overflow
  /// and lead to undefined behaviour. Unable to comply is UB.
  pub unsafe fn get_raw(&self) -> Option<&'static T> {
    let old = self.lock.fetch_add(1, Ordering::Acquire);

    if old & PINNED == 0 {
      self.lock.fetch_sub(1, Ordering::Release);
      return None;
    }

    Some(&unsafe { &*self.code.load(Ordering::Acquire) }.code)
  }

  /// WARNING: Multiple writers is undefined behaviour
  ///
  /// flags: <reserved> [0: PINNED]
  pub unsafe fn set(&self, flags: u8, data: T, tries: Option<NonZeroU32>) -> Option<()> {
    let mut initial = self.lock.load(Ordering::Relaxed);

    let mut iteration = 0;
    loop {
      iteration += 1;

      if tries.map(|x| iteration > x.get()).unwrap_or(false) {
        return None;
      }

      if initial & PINNED > 0 {
        return None;
      }

      // If there are readers, spin_loop
      if initial & MASK > 0 {
        initial = self.lock.load(Ordering::Relaxed);
        spin_loop();

        continue;
      }

      match self.lock.compare_exchange_weak(
        initial,
        initial | LOCKED,
        Ordering::Acquire,
        Ordering::Relaxed,
      ) {
        Ok(_) => break,
        // Enlighten us
        Err(new) => {
          initial = new;
          continue;
        }
      }
    }

    // CRITICAL SECTION
    let ptr = Box::into_raw(Box::new(StoredCode {
      refcount: AtomicU32::new(1),
      code: data,
    }));
    let old_ptr = self.code.swap(ptr, Ordering::AcqRel);
    unsafe { CodeGuard::dec(old_ptr) };
    // CRITICAL SECTION END

    _ = self
      .lock
      .fetch_update(Ordering::Release, Ordering::Acquire, |old| {
        // Remove locked, remove LOCKED data
        let flags_mask = ((flags as u32) << SH_AMT) & PINNED;
        Some(old & !LOCKED | flags_mask)
      });

    Some(())
  }
}

impl<T> Drop for SwappableCodeStore<T> {
  fn drop(&mut self) {
    // Unless it is perfectly okay to, we do not even think of decrementing the guard
    if self.lock.load(Ordering::Acquire) & MASK == 0 {
      unsafe { CodeGuard::dec(self.code.load(Ordering::Acquire)) };
    }
  }
}

pub struct StoredCode<T> {
  refcount: AtomicU32,
  code: T,
}

pub struct CodeGuard<T>(*mut StoredCode<T>);

impl<T> CodeGuard<T> {
  /// Increments count, keeps things working
  fn new(r: *mut StoredCode<T>) -> Self {
    unsafe { &*r }.refcount.fetch_add(1, Ordering::Relaxed);

    Self(r)
  }

  pub fn reference<'a>(&'a self) -> &'a T {
    self.deref()
  }

  /// ## Safety
  ///
  /// This function should also be accompanied by [Self::from_raw] to reconstruct the structure
  /// to ensure that [Drop] semantics are correctly called
  pub unsafe fn into_raw(self) -> *const T {
    let p = self.0 as *mut u8;
    forget(self);

    unsafe { p.byte_offset(offset_of!(StoredCode<T>, code) as _) as _ }
  }

  /// # Safety
  ///
  /// The pointer must and only must come from an accompanying [Self::into_raw] without any manual
  /// offsets being applied
  pub unsafe fn from_raw(ptr: *const T) -> Self {
    Self(unsafe {
      (ptr as *const u8).byte_offset(-(offset_of!(StoredCode<T>, code) as isize))
        as *mut StoredCode<T>
    })
  }

  // Decrement
  unsafe fn dec(r: *mut StoredCode<T>) {
    let old = unsafe { &*r }.refcount.fetch_sub(1, Ordering::AcqRel);

    // Drop time!
    if old == 1 {
      unsafe { drop(Box::from_raw(r)) };
    }
  }
}

impl<T> Drop for CodeGuard<T> {
  fn drop(&mut self) {
    unsafe {
      Self::dec(self.0);
    }
  }
}

impl<T> Deref for CodeGuard<T> {
  type Target = T;

  fn deref(&self) -> &Self::Target {
    &unsafe { &*self.0 }.code
  }
}
