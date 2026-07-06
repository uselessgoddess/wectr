#![cfg_attr(not(test), no_std)]
#![allow(unsafe_op_in_unsafe_fn)]

pub mod channel;
pub mod rt;

extern crate alloc;

use core::slice;

pub use channel::{Emit, Inbound, Outbound, Pod, Receiver};
pub use rt::{Runtime, spawn_local as spawn, wait};

#[doc(hidden)]
pub fn __start<F: Future<Output = ()> + 'static>(future: F) {
  rt::init(Runtime::new(future));
}

#[doc(hidden)]
pub fn __tick(elapsed_ms: i64) -> i32 {
  rt::current().tick(elapsed_ms as u64) as i32
}

/// # Safety
/// `ptr`/`len` must describe a readable region of guest memory.
#[doc(hidden)]
pub unsafe fn __dispatch(channel: u32, ptr: *const u8, len: usize) {
  if let Some(rt) = rt::try_current() {
    let bytes = slice::from_raw_parts(ptr, len).to_vec();
    rt.dispatch(channel, bytes);
  }
}

#[macro_export]
macro_rules! entry {
  ($main:path) => {
    #[unsafe(no_mangle)]
    pub extern "C" fn wectr_start() {
      $crate::__start($main());
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn wectr_tick(elapsed_ms: i64) -> i32 {
      $crate::__tick(elapsed_ms)
    }
    #[unsafe(no_mangle)]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub extern "C" fn wectr_dispatch(channel: u32, ptr: *const u8, len: usize) {
      unsafe { $crate::__dispatch(channel, ptr, len) };
    }
  };
}

#[cfg(feature = "guest")]
#[global_allocator]
static ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

#[cfg(all(feature = "guest", target_arch = "wasm32"))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
  core::arch::wasm32::unreachable()
}

#[cfg(test)]
mod tests {
  use alloc::rc::Rc;
  use core::cell::Cell;
  use std::sync::Mutex;

  use super::*;
  use crate::channel::{self, Channel};

  static RT_GUARD: Mutex<()> = Mutex::new(());
  fn rt_guard() -> std::sync::MutexGuard<'static, ()> {
    RT_GUARD.lock().unwrap_or_else(|e| e.into_inner())
  }

  channels! {
      inbound {
          7 => struct Resupply { rounds: u32 }
      }
      outbound {
          2 => struct Shoot { target: u32 }
      }
  }

  #[test]
  fn concurrency() {
    let _guard = rt_guard();
    let apples = Rc::new(Cell::new(0u32));
    let bananas = Rc::new(Cell::new(0u32));
    let (a, b) = (apples.clone(), bananas.clone());

    __start(async move {
      let s2 = a.clone();
      spawn(async move {
        for _ in 0..3 {
          s2.set(s2.get() + 1);
          wait(150).await;
        }
      });
      for _ in 0..5 {
        b.set(b.get() + 1);
        wait(100).await;
      }
    });

    let mut concurrently = false;
    for _ in 0..1000 {
      if apples.get() > 0 && bananas.get() > 0 && apples.get() < 3 {
        concurrently = true;
      }
      if __tick(50) == 0 {
        break;
      }
    }
    assert_eq!(apples.get(), 3);
    assert_eq!(bananas.get(), 5);
    assert!(concurrently, "tasks did not overlap in time");
  }

  #[test]
  fn channel_round_trip() {
    let _guard = rt_guard();
    channel::host::EMITS.with(|e| e.borrow_mut().clear());
    channel::host::SUBS.with(|s| s.borrow_mut().clear());
    channel::host::UNSUBS.with(|s| s.borrow_mut().clear());

    __start(async {
      let mut resupply = Resupply::subscribe();
      let got = resupply.recv().await;
      Shoot { target: got.rounds }.emit();
    });

    assert_eq!(__tick(0), 1);
    assert_eq!(
      channel::host::SUBS.with(|s| s.borrow().clone()),
      vec![Resupply::ID]
    );

    let msg = Resupply { rounds: 42 };
    let bytes = channel::as_bytes(&msg).to_vec();
    unsafe { __dispatch(Resupply::ID, bytes.as_ptr(), bytes.len()) };

    assert_eq!(__tick(0), 0);

    channel::host::EMITS.with(|e| {
      let emits = e.borrow();
      assert_eq!(emits.len(), 1);
      assert_eq!(emits[0].0, Shoot::ID);
      assert_eq!(
        channel::from_bytes::<Shoot>(&emits[0].1),
        Shoot { target: 42 }
      );
    });
    assert_eq!(
      channel::host::UNSUBS.with(|s| s.borrow().clone()),
      vec![Resupply::ID]
    );
  }

  #[test]
  fn slice_round_trip() {
    let _guard = rt_guard();
    channel::host::EMITS.with(|e| e.borrow_mut().clear());

    let shots = [Shoot { target: 1 }, Shoot { target: 2 }, Shoot { target: 3 }];
    shots.emit();

    channel::host::EMITS.with(|e| {
      let emits = e.borrow();
      assert_eq!(emits.len(), 1);
      assert_eq!(emits[0].0, Shoot::ID);
      assert_eq!(emits[0].1, channel::slice_bytes(&shots));
    });
  }
}
