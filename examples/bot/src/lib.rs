#![no_std]

extern crate alloc;

use wectr_sdk::{Emit, Inbound, channels, wait};

channels! {
    inbound {
        0 => struct Ammo { rounds: u32 }
    }
    outbound {
        0 => struct Move { dx: i32, dy: i32 }
        1 => struct Hide {}
    }
}

async fn bot() {
  let mut ammo = Ammo::subscribe();
  loop {
    let Ammo { rounds } = ammo.recv().await;
    if rounds == 0 {
      break;
    }
    Move { dx: 1, dy: 0 }.emit();
    wait(100).await;
  }
  Hide {}.emit();
}

wectr_sdk::entry!(bot);
