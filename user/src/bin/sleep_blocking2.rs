#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;
extern crate alloc;

use user_lib::{fork, getpid, get_time, sleep_blocking, waitpid};

const CASE_NUM: usize = 4;

// 故意乱序设置 sleep 时间。
// spawn/fork 创建顺序是 100 -> 10 -> 50 -> 20，
// 但理想返回顺序应该大致是 10 -> 20 -> 50 -> 100。
const TARGETS_MS: [usize; CASE_NUM] = [100, 10, 50, 20];

#[no_mangle]
pub fn main() -> i32 {
    println!("[sleep order parent] pid = {}", getpid());

    let mut child_pids = [0usize; CASE_NUM];

    for i in 0..CASE_NUM {
        let pid = fork();

        if pid == 0 {
            // child
            let target_ms = TARGETS_MS[i];
            let child_pid = getpid();

            println!(
                "[sleep order child start] pid = {}, index = {}, target = {} ms",
                child_pid, i, target_ms
            );

            let start = get_time();
            sleep_blocking(target_ms);
            let end = get_time();

            let elapsed = end - start;
            let error = elapsed as isize - target_ms as isize;

            println!(
                "[sleep order result] pid = {}, index = {}, target = {} ms, start = {} ms, end = {} ms, elapsed = {} ms, error = {} ms",
                child_pid,
                i,
                target_ms,
                start,
                end,
                elapsed,
                error
            );

            return 0;
        } else {
            // parent
            child_pids[i] = pid as usize;
            // println!(
            //     "[sleep order parent] fork child index = {}, pid = {}, target = {} ms",
            //     i, child_pids[i], TARGETS_MS[i]
            // );
        }
    }

    let mut exit_code: i32 = 0;
    for i in 0..CASE_NUM {
        waitpid(child_pids[i], &mut exit_code);
        println!(
            "[sleep order parent] child pid = {} exited with code = {}",
            child_pids[i], exit_code
        );
    }

    println!("[sleep order parent] Test sleep order finished!");
    0
}