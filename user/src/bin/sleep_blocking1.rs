#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;
extern crate alloc;

use user_lib::{getpid, get_time, sleep_blocking, spawn, waitpid};

#[no_mangle]
pub fn main() -> i32 {
    println!("[sleep blocking 1] from pid: {}", getpid());
    const SLEEP_PROCESS_NUM: usize = 20;
    let sleep_blocking_pid: [usize; SLEEP_PROCESS_NUM] =
        array_init::array_init(|_| spawn("sleep_blocking\0") as usize);
    let mut exit_code: i32 = 0;
    for i in 0..SLEEP_PROCESS_NUM {
        waitpid(sleep_blocking_pid[i], &mut exit_code);
    }
    println!("[sleep blocking 1] Test sleep blocking 1 finished!");
    0
}
