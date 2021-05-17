use crayfish::global_id;
use crayfish::global_id::here;
use crayfish::global_id::world_size;
use crayfish::logging::*;
use crayfish::place::Place;
use rand::seq::SliceRandom;
use rand::SeedableRng;

extern crate crayfish;
extern crate rand;

#[crayfish::activity]
async fn quick_sort(
    mut nums: Vec<usize>,
) -> Vec<usize> {
        info!("sorting vector of len: {}", nums.len());
        if nums.len() < 10 {
            nums.sort();
            return nums;
        }
        let pivot = nums[0];
        let rest = &nums[1..];
        let left: Vec<_> = rest.iter().filter(|n| **n < pivot).cloned().collect();
        let right: Vec<_> = rest.iter().filter(|n| **n >= pivot).cloned().collect();

        let neighbor = (here() as usize + 1) % world_size();

        // wait for result of sub activities
        let left_sorted_future = crayfish::at!(neighbor as Place, quick_sort(left));
        let right_sorted_future = crayfish::at!(here(), quick_sort(right));
        // overlap the local & remote computing
        let right_sorted = right_sorted_future.await;
        let mut left_sorted = left_sorted_future.await;
        left_sorted.push(pivot);
        left_sorted.extend_from_slice(&right_sorted[..]);
        left_sorted
}

#[crayfish::main]
async fn main() -> Result<(), std::io::Error> {
    Ok(crayfish::finish!{
        if global_id::here() == 0 {
            // ctx contains a new finish id now
            let mut rng = rand::rngs::StdRng::from_entropy();
            let mut nums: Vec<usize> = (0..1000).collect();
            nums.shuffle(&mut rng);
            info!("before sorting: {:?}", nums);
            let sorted = crayfish::at!(here(), quick_sort(nums)).await;
            info!("sorted: {:?}", sorted);

            info!("Main finished");
        }
    })
}
