use crayfish::collective;
use crayfish::global_id;
use crayfish::global_id::world_size;
use crayfish::inventory;
use crayfish::logging::*;
use crayfish::place::Place;
use crayfish::shared::PlaceLocal;
use crayfish::shared::PlaceLocalWeak;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs::File;
use std::hash::Hash;
use std::hash::Hasher;
use std::io::BufRead;
use std::io::BufReader;
use std::sync::Mutex;
use crayfish::finish;
use crayfish::ff;

extern crate crayfish;
extern crate futures;

type CountNumber = u32;
type KMer = Vec<u8>;
type Reads = Vec<Vec<u8>>;

const KMER_LEN: usize = 31;

const BASE_A: u8 = b'A';
const BASE_T: u8 = b'T';
const BASE_C: u8 = b'C';
const BASE_G: u8 = b'G';

#[crayfish::arg]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct KMerData {
    count: CountNumber,
}

type CountTable = HashMap<KMer, KMerData>;

fn get_complement_base(base: u8) -> u8 {
    match base {
        BASE_A => BASE_T,
        BASE_T => BASE_A,
        BASE_C => BASE_G,
        BASE_G => BASE_C,
        other => panic!("unknown base: {}", other),
    }
}

fn get_canonical(kmer: KMer) -> (KMer, bool) {
    // bool is true if there is a change
    use std::cmp::Ordering::*;
    for i in 0..kmer.len() / 2 + 1 {
        match kmer[i].cmp(&get_complement_base(kmer[kmer.len() - 1 - i])) {
            Less => return (get_reverse_complement(&kmer), true),
            Greater => return (kmer, false),
            Equal => continue,
        }
    }
    panic!("KMer is plalindrome")
}

fn get_reverse_complement(kmer: &KMer) -> KMer {
    kmer.iter()
        .rev()
        .map(|x| get_complement_base(*x))
        .clone()
        .collect()
}

fn update_count_table(count_table: &mut CountTable, kmer: KMer, kmerdata: KMerData) {
    // merge data of the same kmer
    match count_table.get_mut(&kmer) {
        Some(kd) => {
            kd.count += kmerdata.count;
        }
        None => {
            count_table.insert(kmer, kmerdata);
        }
    };
}

fn get_partition(kmer: &KMer) -> Place {
    let mut hasher = DefaultHasher::new();
    kmer.hash(&mut hasher);
    (hasher.finish() % global_id::world_size() as u64) as Place
}

#[crayfish::activity]
async fn update_kmer(kmer: KMer, data: KMerData, final_ptr: PlaceLocalWeak<Mutex<CountTable>>) {
    let ptr = final_ptr.upgrade().unwrap();
    let mut h = ptr.lock().unwrap();
    update_count_table(&mut h, kmer, data);
}

#[crayfish::activity]
async fn kmer_counting(reads: Reads, final_ptr: PlaceLocalWeak<Mutex<CountTable>>) {
    let mut count_table = CountTable::new();

    for read in reads {
        // drop too short read
        if read.len() < KMER_LEN {
            continue;
        }
        // drop read with unknown base
        if read
            .iter()
            .any(|b| *b != BASE_A && *b != BASE_T && *b != BASE_C && *b != BASE_G)
        {
            continue;
        }

        // TODO: further optimization
        for start_pos in 0..(read.len() - KMER_LEN + 1) {
            let kmer = Vec::from(&read[start_pos..start_pos + KMER_LEN]);
            let (canonical, _) = get_canonical(kmer);
            let kmer_data = KMerData { count: 1 };

            update_count_table(&mut count_table, canonical, kmer_data);
        }
    }

    for (k, n) in count_table {
        crayfish::ff!(get_partition(&k), update_kmer(k, n, final_ptr.clone()));
    }
}

// desugered finish
#[crayfish::main]
async fn inner_main() {
    let count_table_ptr = PlaceLocal::new(Mutex::new(CountTable::new()));
    collective::barrier();
    if global_id::here() == 0 {
        // ctx contains a new finish id now
        let chunk_size = 4096;
        let args = std::env::args().collect::<Vec<_>>();
        let filename = &args[1];
        let file = File::open(filename).unwrap();
        let lines = BufReader::new(file).lines();

        let world_size = world_size();
        let mut next_place: Place = 0;
        let mut buffer: Reads = vec![];

        finish! {
        for (l_num, line) in lines.enumerate() {
            if l_num % 4 != 1 {
                continue;
            }
            if let Ok(s) = line {
                if buffer.len() == chunk_size {
                    warn!(
                        "Sending {}~{} reads to {}",
                        l_num / 4,
                        l_num / 4 + chunk_size - 1,
                        next_place
                    );
                    let mut new_read = vec![];
                    std::mem::swap(&mut new_read, &mut buffer);
                    ff!(next_place, kmer_counting(new_read, count_table_ptr.downgrade()));
                    next_place = (next_place + 1) % (world_size as Place);
                }
                buffer.push(s.into_bytes());
            }
        }
        }
    }
    collective::barrier();

    let global_table = count_table_ptr.lock().unwrap();

    let p = global_table.iter().take(100).collect::<Vec<_>>();
    for (kmer, data) in p {
        println!("{}: {}", String::from_utf8_lossy(&kmer[..]), data.count);
    }

    let mut hist = vec![0usize; 2048];
    for (_, data) in global_table.iter() {
        if data.count as usize > hist.len() {
            for _ in 0..data.count {
                hist.push(0);
            }
        }
        hist[data.count as usize] += 1;
    }
    println!("{:?}", hist);
}
