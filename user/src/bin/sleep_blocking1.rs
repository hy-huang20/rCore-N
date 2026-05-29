#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;
extern crate alloc;

use user_lib::{send_msg, getpid, get_time, sleep_blocking, spawn, waitpid};

#[no_mangle]
pub fn main() -> i32 {
    println!("[sleep blocking 1] from pid: {}", getpid());
    const CPU_LOAD_NUM: usize = 4;
    let cpu_load_pid: [usize; CPU_LOAD_NUM] =
        array_init::array_init(|_| spawn("cpu_load\0") as usize);
    let mut exit_code: i32 = 0;

    const SLEEP_PROCESS_NUM: usize = 10;
    let sleep_blocking_pid: [usize; SLEEP_PROCESS_NUM] =
        array_init::array_init(|_| spawn("sleep_blocking\0") as usize);

    for i in 0..SLEEP_PROCESS_NUM {
        waitpid(sleep_blocking_pid[i], &mut exit_code);
    }

    for i in cpu_load_pid {
        send_msg(i, 15);
        waitpid(i, &mut exit_code);
    }
    println!("[sleep blocking 1] Test sleep blocking 1 finished!");
    0
}
