use std::{
  hint::{cold_path, spin_loop},
  num::NonZeroU32,
  ops::Deref,
  sync::atomic::{AtomicPtr, AtomicU32, Ordering},
};

pub struct SwappableCodeStore<T> {
  // [31: LOCKED] [30: PINNED] [29: JIT] <readers count>
  lock: AtomicU32,
  code: AtomicPtr<StoredCode<T>>,
}

const LOCKED: u32 = 1 << 31;
const PINNED: u32 = 1 << 30;
const JIT: u32 = 1 << 29;

const SH_AMT: u32 = 29;

pub const U8_JIT: u8 = 1 << 0;
pub const U8_PINNED: u8 = 1 << 1;

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
  /// <reserved> [1: PINNED] [0: JIT]
  pub fn get(&self) -> (u8, CodeGuard<T>) {
    loop {
      let old = self.lock.fetch_add(1, Ordering::Acquire);

      // We were right!
      if old & LOCKED == 0 {
        let out = CodeGuard::new(self.code.load(Ordering::Acquire));

        let old = self.lock.fetch_sub(1, Ordering::Release);

        return (((old & (PINNED | JIT)) >> SH_AMT) as _, out);
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

  /// WARNING: Multiple writers is undefined behaviour
  ///
  /// flags: <reserved> [1: PINNED] [0: JIT]
  pub unsafe fn set(&self, flags: u8, data: T, tries: Option<NonZeroU32>) -> Option<()> {
    let mut initial = self.lock.load(Ordering::Relaxed);

    let mut iteration = 0;
    loop {
      iteration += 1;

      if tries.map(|x| iteration > x.get()).unwrap_or(false) {
        return None;
      }

      // If there are readers, spin_loop
      if initial & !LOCKED > 0 {
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
        let flags_mask = ((flags as u32) << SH_AMT) & (JIT | PINNED);
        Some(old & !LOCKED | flags_mask)
      });

    Some(())
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
