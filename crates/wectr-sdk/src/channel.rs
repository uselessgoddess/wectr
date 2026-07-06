use core::{
  marker::PhantomData,
  mem::{MaybeUninit, size_of, size_of_val},
  pin::Pin,
  ptr, slice,
  task::{Context, Poll},
};

use crate::rt;

/// # Safety
/// Every bit pattern must be a valid `T`: `#[repr(C)]`.
pub unsafe trait Pod: Copy {}

macro_rules! pod_prims {
    ($($t:ty)*) => { $( unsafe impl Pod for $t {} )* };
}
pod_prims!(() u8 u16 u32 u64 u128 i8 i16 i32 i64 i128 f32 f64 usize isize);

unsafe impl<T: Pod, const N: usize> Pod for [T; N] {}

pub trait Channel: Pod {
  const ID: u32;
}

pub trait Outbound: Channel {}

pub trait Inbound: Channel + Sized {
  fn subscribe() -> Receiver<Self> {
    Receiver::new()
  }
}

pub trait Emit {
  fn emit(&self) -> i64;
}

impl<T: Outbound> Emit for T {
  fn emit(&self) -> i64 {
    host::emit(T::ID, as_bytes(self))
  }
}

impl<T: Outbound> Emit for [T] {
  fn emit(&self) -> i64 {
    host::emit(T::ID, slice_bytes(self))
  }
}

pub fn as_bytes<T: Pod>(value: &T) -> &[u8] {
  unsafe {
    slice::from_raw_parts((value as *const T).cast::<u8>(), size_of::<T>())
  }
}

pub fn slice_bytes<T: Pod>(items: &[T]) -> &[u8] {
  unsafe {
    slice::from_raw_parts(items.as_ptr().cast::<u8>(), size_of_val(items))
  }
}

pub fn from_bytes<T: Pod>(bytes: &[u8]) -> T {
  let mut val = MaybeUninit::<T>::zeroed();
  let n = bytes.len().min(size_of::<T>());
  unsafe {
    ptr::copy_nonoverlapping(bytes.as_ptr(), val.as_mut_ptr().cast::<u8>(), n);
    val.assume_init()
  }
}

pub struct Receiver<T: Inbound>(PhantomData<fn(T)>);

impl<T: Inbound> Receiver<T> {
  fn new() -> Self {
    host::subscribe(T::ID);
    Self(PhantomData)
  }

  pub fn recv(&mut self) -> Recv<T> {
    Recv(PhantomData)
  }
}

impl<T: Inbound> Drop for Receiver<T> {
  fn drop(&mut self) {
    host::unsubscribe(T::ID);
    rt::current().drain(T::ID);
  }
}

pub struct Recv<T: Inbound>(PhantomData<fn() -> T>);

impl<T: Inbound> Future for Recv<T> {
  type Output = T;
  fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<T> {
    match rt::current().take(T::ID) {
      Some(bytes) => Poll::Ready(from_bytes(&bytes)),
      None => Poll::Pending,
    }
  }
}

#[macro_export]
macro_rules! channels {
  ($(
    $(#[$dmeta:meta])*
    $dir:ident {
      $(
        $(#[$smeta:meta])*
        $id:literal => struct $name:ident {
          $( $(#[$fmeta:meta])* $field:ident : $fty:ty ),* $(,)?
        }
      )*
    }
  )*) => {
    $($(
      $(#[$smeta])*
      #[repr(C)]
      #[derive(Clone, Copy, Debug, PartialEq)]
      pub struct $name {
        $( $(#[$fmeta])* pub $field : $fty ),*
      }

      const _: fn() = || {
        fn assert_pod<T: $crate::channel::Pod>() {}
        $( assert_pod::<$fty>(); )*
      };

      unsafe impl $crate::channel::Pod for $name {}
      impl $crate::channel::Channel for $name {
        const ID: u32 = $id;
      }
      $crate::channels!(@dir $dir $name);
    )*)*
  };

  (@dir inbound $name:ident) => {
      impl $crate::channel::Inbound for $name {}
  };
  (@dir outbound $name:ident) => {
      impl $crate::channel::Outbound for $name {}
  };
}

#[cfg(target_arch = "wasm32")]
mod host {
  #[link(wasm_import_module = "wectr")]
  unsafe extern "C" {
    fn wectr_emit(channel: u32, ptr: *const u8, len: usize) -> i64;
    fn wectr_subscribe(channel: u32);
    fn wectr_unsubscribe(channel: u32);
  }

  pub fn emit(channel: u32, data: &[u8]) -> i64 {
    unsafe { wectr_emit(channel, data.as_ptr(), data.len()) }
  }
  pub fn subscribe(channel: u32) {
    unsafe { wectr_subscribe(channel) }
  }
  pub fn unsubscribe(channel: u32) {
    unsafe { wectr_unsubscribe(channel) }
  }
}

#[cfg(all(not(target_arch = "wasm32"), not(test)))]
mod host {
  pub fn emit(_channel: u32, _data: &[u8]) -> i64 {
    0
  }
  pub fn subscribe(_channel: u32) {}
  pub fn unsubscribe(_channel: u32) {}
}

#[cfg(test)]
pub(crate) mod host {
  use std::cell::RefCell;

  thread_local! {
      pub static EMITS: RefCell<Vec<(u32, Vec<u8>)>> = const { RefCell::new(Vec::new()) };
      pub static SUBS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
      pub static UNSUBS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
  }

  pub fn emit(channel: u32, data: &[u8]) -> i64 {
    EMITS.with(|e| e.borrow_mut().push((channel, data.to_vec())));
    0
  }
  pub fn subscribe(channel: u32) {
    SUBS.with(|s| s.borrow_mut().push(channel));
  }
  pub fn unsubscribe(channel: u32) {
    UNSUBS.with(|s| s.borrow_mut().push(channel));
  }
}
