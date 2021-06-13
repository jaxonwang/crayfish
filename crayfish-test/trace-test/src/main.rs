use std::time;
use std::thread;

#[crayfish::main]
async fn main() {
    let task = ||{
    let duration = time::Duration::from_micros(1);
    for _ in 0..1000{
        crayfish::profiling_start!("trace-test0");
        thread::sleep(duration);
        crayfish::profiling_stop!();

        crayfish::profiling_start!("trace-test1");
        thread::sleep(duration);
        crayfish::profiling_stop!();
    }
    println!("done")
    };

    let t0 = thread::spawn(task.clone());
    let t1= thread::spawn(task);
    t0.join().unwrap();
    t1.join().unwrap();
    crayfish::trace::print_profiling();

}
