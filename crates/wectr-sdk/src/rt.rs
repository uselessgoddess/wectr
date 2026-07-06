use alloc::{
  boxed::Box,
  collections::{BTreeMap, VecDeque},
  vec::Vec,
};
use core::{
  cell::{Cell, RefCell},
  future::Future,
  mem,
  pin::Pin,
  ptr,
  task::{Context, Poll, Waker},
};

type Task = Pin<Box<dyn Future<Output = ()>>>;

pub struct Runtime {
  tasks: RefCell<Vec<Task>>,
  spawned: RefCell<Vec<Task>>,
  clock: Cell<u64>,
  inboxes: RefCell<BTreeMap<u32, VecDeque<Vec<u8>>>>,
}

impl Runtime {
  pub fn new<F: Future<Output = ()> + 'static>(future: F) -> Self {
    Self {
      tasks: RefCell::new(alloc::vec![Box::pin(future)]),
      spawned: RefCell::new(Vec::new()),
      clock: Cell::new(0),
      inboxes: RefCell::new(BTreeMap::new()),
    }
  }

  pub fn spawn(&self, task: Task) {
    self.spawned.borrow_mut().push(task);
  }

  pub fn dispatch(&self, channel: u32, data: Vec<u8>) {
    self.inboxes.borrow_mut().entry(channel).or_default().push_back(data);
  }

  pub fn take(&self, channel: u32) -> Option<Vec<u8>> {
    self.inboxes.borrow_mut().get_mut(&channel)?.pop_front()
  }

  pub fn drain(&self, channel: u32) {
    self.inboxes.borrow_mut().remove(&channel);
  }

  pub fn tick(&self, elapsed_ms: u64) -> bool {
    self.clock.set(self.clock.get().saturating_add(elapsed_ms));

    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    loop {
      let mut tasks = mem::take(&mut *self.tasks.borrow_mut());
      tasks.retain_mut(|task| task.as_mut().poll(&mut cx).is_pending());

      let spawned = mem::take(&mut *self.spawned.borrow_mut());
      let admitted = !spawned.is_empty();
      tasks.extend(spawned);
      *self.tasks.borrow_mut() = tasks;

      if !admitted {
        return !self.tasks.borrow().is_empty();
      }
    }
  }

  fn now(&self) -> u64 {
    self.clock.get()
  }
}

static mut RT: Option<Runtime> = None;

pub(crate) fn current() -> &'static Runtime {
  try_current().expect("wectr_start was not called before this entry point")
}

pub(crate) fn try_current() -> Option<&'static Runtime> {
  unsafe { (*ptr::addr_of!(RT)).as_ref() }
}

pub(crate) fn init(rt: Runtime) {
  unsafe { *ptr::addr_of_mut!(RT) = Some(rt) }
}

pub fn spawn_local<F: Future<Output = ()> + 'static>(future: F) {
  current().spawn(Box::pin(future));
}

pub fn wait(ms: u64) -> Wait {
  Wait { deadline: None, ms }
}

pub struct Wait {
  deadline: Option<u64>,
  ms: u64,
}

impl Future for Wait {
  type Output = ();
  fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
    let now = current().now();
    let this = self.get_mut();
    let deadline = *this.deadline.get_or_insert(now + this.ms);
    if now >= deadline { Poll::Ready(()) } else { Poll::Pending }
  }
}
