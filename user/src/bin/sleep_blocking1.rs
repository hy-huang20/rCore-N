#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;
extern crate alloc;

use user_lib::{getpid, get_time, sleep_blocking, spawn, waitpid};

#[no_mangle]
pub fn main() -> i32 {
    println!("[sleep blocking 1] from pid: {}", getpid());
    let mut exit_code: i32 = 0;
    let pid1 = spawn("sleep_blocking\0") as usize;
    let pid2 = spawn("sleep_blocking\0") as usize;
    let pid3 = spawn("sleep_blocking\0") as usize;
    let pid4 = spawn("sleep_blocking\0") as usize;
    waitpid(pid1, &mut exit_code);
    waitpid(pid2, &mut exit_code);
    waitpid(pid3, &mut exit_code);
    waitpid(pid4, &mut exit_code);
    println!("[sleep blocking 1] Test sleep blocking 1 finished!");
    0
}
