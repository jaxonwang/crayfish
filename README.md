<p align="center">
  <img src="https://user-images.githubusercontent.com/10634580/159606525-0eca91c8-96ef-4e19-bece-524861896efd.png" width="350" />
</p>

# Crayfish
[![Crayfish crate](https://img.shields.io/crates/v/crayfish.svg)](https://crates.io/crates/crayfish)
[![Crayfish documentation](https://docs.rs/crayfish/badge.svg)](https://docs.rs/crayfish)
[![3-Clause licensed](https://img.shields.io/badge/license-BSD-blue.svg)](https://github.com/jaxonwang/crayfish/blob/master/LICENSE)
[![Build Status](https://github.com/jaxonwang/crayfish/workflows/CI/badge.svg)](https://github.com/jaxonwang/crayfish/actions/workflows/rust.yml)

Crayfish brings the APGAS programming model to the Rust programming language. The goal of Crayfish is to enable better productivity of parallel programming by porting key concepts of APGAS programming language [X10][x10-url]. It is:

- **Fast**: The framework is lightweight. It provides a high-level abstraction for parallel programming with little extra CPU, memory, and network overheads.
- **Productivity**: Rust's concepts like lifetime and ownership now cross the boundary of a single process! They help you write safe and correct parallel programs.
- **Scalable**: Crayfish can handle large-scale data processing at the scales of a thousand CPUs.

[X10-url]: http://x10-lang.org/

## What is APGAS?
APGAS (Asynchronous Partitioned Global Address Space) is a parallel programming model targeting the productivity of parallel programming. It was first implemented by the programming language X10. More details can be found on [wiki][gpas-url]. When it comes to Crayfish, Rust's async/await now is extended to a remote process by using Crayfish's macro `at`: 
[//]: # (TODO: link to the API doc)
```rust
use crayfish;
let process_id = crayfish::place::here() +1; // get a neighbor process;
let x = crayfish::at!(process_id, async_foo(1)).await;
```
Given a process id (Place), async function, and proper parameters, Crayfish return a future of the remote async function invocation. You can await the future to get the return. Exactly the same manner how you use async/await!

[gpas-url]: https://en.wikipedia.org/wiki/Partitioned_global_address_space

## Requirements
- A POSIX-like environment
- GNU autoconf & GNU automake
- GNU make (version 3.79 or newer)
- [requirement for bindgen](https://rust-lang.github.io/rust-bindgen/requirements.html)

## Usage 

Add a line into your Cargo.toml:
```toml
[dependencies]
crayfish = "0.01"
```
Crayfish requires rustc  >= 1.51.0

Let's look at a simple example of parallel quick sort:
```rust
use crayfish::place::here;
use crayfish::place::Place;

#[crayfish::activity]
async fn quick_sort(nums: Vec<usize>) -> Vec<usize> {
        if nums.len() <= 1 {
            return nums;
        }
        let pivot = nums[0];
        let rest = &nums[1..];
        // put all numbers smaller than the pivot into the left vector
        let left: Vec<_> = rest.iter().filter(|n| **n < pivot).cloned().collect();
        // put all numbers greater than or equal to the pivot into the right vector
        let right: Vec<_> = rest.iter().filter(|n| **n >= pivot).cloned().collect();

        // recursively call to sort left part at a remote process
        let neighbor = (here() as usize + 1) % crayfish::place::world_size();
        let left_sorted_future = crayfish::at!(neighbor as Place, quick_sort(left));
        // recursively call to sort right
        let right_sorted_future = crayfish::at!(here(), quick_sort(right));
        // overlap the local & remote computing
        let right_sorted = right_sorted_future.await; // get left sorted vector
        let mut left_sorted = left_sorted_future.await; // get right sorted vector
        // combine the sorted
        left_sorted.push(pivot);
        left_sorted.extend_from_slice(&right_sorted[..]);
        left_sorted
}

#[crayfish::main]
async fn main() {
    crayfish::finish!{
        if here() == 0 {
            let mut nums: Vec<usize> = vec![9,5,7,1,6,4,7,8,1];
            let sorted = crayfish::at!(here(), quick_sort(nums)).await;
            println!("sorted: {:?}", sorted);
        }
    }
}
```

## Tutorial
Comming soon.

## API Documention
[here](https://docs.rs/crayfish)

## Networks

For networking in a high-performance environment, Crayfish use [GASNet-EX](https://gasnet.lbl.gov/), the most widely used PGAS network library. Crayfish depends on [rust-bindgen](https://github.com/rust-lang/rust-bindgen) to dynamically generate GASNet-EX library bindings for Rust. Please make sure you meet the [requirement for bindgen](https://rust-lang.github.io/rust-bindgen/requirements.html). Such limitation will be removed by replacing a static binding in a future release. 

Crayfish calls system default c/c++ compiler to build GASNet-EX. Following environment variables should work for GASNet-EX building::
```sh
export CC='your c compiler'
export CFLAGS='cflags'
export LDFLAGS='ldflags'
export LIBS='libs'
export CPPFLAGS='cppflags'
export CPP='c preprocessor'
export CXX='your c++ compiler'
export CXXFLAGS='c++ flags'
export CXXCPP='c++ preprocessor'
```
### Use Crayfish with UDP Conduit
By default, `cargo build` an application will select UDP conduit of GASNet-EX. To use UDP conduit, please add the following lines to your Cargo.toml:
```toml
[dependencies]
crayfish = { version = "0.0.1" }
```
or
```toml
[dependencies]
crayfish = { version = "0.0.1",  features = ["net-udp"] }
```
This approach is the best choice for portability. To exchange necessary information during parallel application set-up, GASNet-EX requires computing nodes have passwordless SSH access to each other. Please configure accordingly if you want to use the UDP conduit. Otherwise, there would be a prompt asking you to type a password.

#### Requirements
A C++98 compiler and POSIX socket library

#### Run
```console
 $ ./a.out <num_nodes> [program args...]
```
For more details of running UDP conduit appliction, please refer [UDP-conduit docs](https://gasnet.lbl.gov/dist-ex/udp-conduit/README).

### MPI
Network over MPI is the most suitable way for Crayfish applications in high-performance environment. To use MPI:
```toml
[dependencies]
crayfish = { version = "0.0.1",  features = ["net-mpi"] }
```
#### Requirements
MPI 1.1 or newer MPI impelmentation.
Crayfish probes the MPI library path by looking at the output of `mpicc -show`. So the MPI implementation should provide `mpicc` wrapper of a c compiler.
You can explicitly specify any of the following environment variables:
```
export MPI_CC='your mpicc',
export MPI_CFLAGS='mpicc cflags'
export MPI_LIBS='paths to mpi libraries'
```
#### Run
```console
$ mpirun [-np <num_nodes>] [program args...]   
```

## License
Crayfish is distributed under BSD-3-Clauses [licence](https://github.com/jaxonwang/crayfish/blob/master/LICENSE).
