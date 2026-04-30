#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;
extern crate alloc;

use user_lib::{getpid, get_time, sleep_blocking};

#[no_mangle]
pub fn main() -> i32 {
    println!("[sleep blocking] from pid: {}", getpid());
    let start = get_time();
    println!("current time_msec = {}", start);
    sleep_blocking(100);
    let end = get_time();
    println!(
        "time_msec = {} after calling sleep_blocking(period_ms: 100), delta = {} ms!",
        end,
        end - start
    );
    println!("Test sleep blocking finished!");
    0
}
