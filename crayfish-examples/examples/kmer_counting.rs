use crayfish::activity::ActivityId;
use crayfish::activity::FunctionLabel;
use crayfish::runtime_meta::FunctionMetaData;
use crayfish::activity::TaskItem;
use crayfish::activity::TaskItemBuilder;
use crayfish::activity::TaskItemExtracter;
use crayfish::essence;
use std::io::BufReader;
use std::io::BufRead;
use std::fs::File;
use crayfish::global_id;
use crayfish::global_id::world_size;
use crayfish::global_id::ActivityIdMethods;
use crayfish::logging::*;
use crayfish::place::Place;
use crayfish::runtime::wait_all;
use crayfish::runtime::wait_single;
use crayfish::runtime::ApgasContext;
use crayfish::runtime::ConcreteContext;
use futures::future::BoxFuture;
use futures::FutureExt;
use std::panic::AssertUnwindSafe;
use crayfish::args::RemoteSend;
use serde::Deserialize;
use std::collections::HashMap;
use std::cmp::Ordering;
use crayfish::inventory;
use serde::Serialize;

extern crate crayfish;
extern crate futures;
extern crate serde;

type CountNumber = u32;
type KMer = Vec<u8>;
type Reads = Vec<Vec<u8>>;

const KMER_LEN:usize = 31;

const BASE_A: u8 = b'A';
const BASE_T: u8 = b'T';
const BASE_C: u8 = b'C';
const BASE_G: u8 = b'G';

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct KMerData {
    count: CountNumber,
}

impl RemoteSend for KMerData{
    type Output = ();
    fn fold(&self, _acc: &mut Self::Output) {
        panic!()
    }
    fn extract(_out: &mut Self::Output) -> Option<Self> {
        panic!()
    }
    fn reorder(&self, _other: &Self) -> Ordering{
        panic!()
    }
    fn is_squashable() -> bool {
        false
    }
}

fn merge_table(to:&mut CountTable, from: CountTable){
    for (kmer, kmerdata) in from{
        if to.contains_key(&kmer){
            to.get_mut(&kmer).unwrap().count += kmerdata.count;
        }else{
            to.insert(kmer, kmerdata);
        }
    }
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

async fn kmer_counting(
    _ctx: & mut impl ApgasContext,
    reads: Reads,
) -> CountTable {
    let mut count_table = CountTable::new();

    for read in reads{

        // drop too short read
        if read.len() < KMER_LEN {
            continue
        }
        // drop read with unknown base
        for b in read.iter() {
            // illumina unknown base
            if *b == b'N' || *b == b'.' {
                continue
            }
        }

        // TODO: further optimization
        for start_pos in 0..(read.len() - KMER_LEN + 1) {
            let kmer = Vec::from(&read[start_pos..start_pos + KMER_LEN]);
            let (canonical, _) = get_canonical(kmer);
            let kmer_data = KMerData { count: 1 };

            update_count_table(&mut count_table, canonical, kmer_data);
        }
    }

    count_table
}

// block until real function finished
async fn execute_and_send_fn0(my_activity_id: ActivityId, waited: bool, reads: Reads) {
    let fn_id: FunctionLabel = 0; // macro
    let finish_id = my_activity_id.get_finish_id();
    let mut ctx = ConcreteContext::inherit(finish_id);
    // ctx seems to be unwind safe
    let future = AssertUnwindSafe(kmer_counting(&mut ctx, reads)); //macro
    let result = future.catch_unwind().await;
    essence::send_activity_result(ctx, my_activity_id, fn_id, waited, result);
}

// the one executed by worker
fn real_fn_wrap_execute_from_remote(item: TaskItem) -> BoxFuture<'static, ()>{
    async move{
    let waited = item.is_waited();
    let mut e = TaskItemExtracter::new(item);
    let my_activity_id = e.activity_id();

    // wait until function return
    trace!(
        "Got activity:{} from {}",
        my_activity_id,
        my_activity_id.get_spawned_place()
    );
    execute_and_send_fn0(my_activity_id, waited, e.arg()).await; // macro
    }.boxed()
}

crayfish::inventory::submit! {
    FunctionMetaData::new(0, real_fn_wrap_execute_from_remote,
                          String::from("kmer_counting"),
                          String::from(file!()),
                          line!(),
                          String::from(module_path!())
                          )
}

// the desugered at async and wait
fn async_create_for_fn_id_0(
    my_activity_id: ActivityId,
    dst_place: Place,
    reads: Reads,
) -> impl futures::Future<Output = CountTable> {
    // macro
    let fn_id: FunctionLabel = 0; // macro

    let f = wait_single(my_activity_id); // macro
    if dst_place == global_id::here() {
        crayfish::spawn(execute_and_send_fn0(my_activity_id, true, reads)); // macro
    } else {
        trace!("spawn activity:{} at place: {}", my_activity_id, dst_place);
        let mut builder = TaskItemBuilder::new(fn_id, dst_place, my_activity_id);
        builder.arg(reads); //macro
        builder.waited();
        let item = builder.build_box();
        ConcreteContext::send(item);
    }
    f
}

// desugered finish
async fn inner_main() {
    if global_id::here() == 0 {
        let mut ctx = ConcreteContext::new_frame();
        // ctx contains a new finish id now
        let mut rets = vec![];
        let chunk_size = 1024;
        let args = std::env::args().collect::<Vec<_>>();
        let filename = &args[1];
        let file = File::open(filename).unwrap();
        let lines = BufReader::new(file).lines();

        let world_size = world_size();
        let mut next_place:Place = 0;
        let mut buffer:Reads = vec![];

        for (l_num, line) in lines.enumerate() {
            if l_num % 4 != 1{
                continue
            }
            if let Ok(s) = line {
                if buffer.len() == chunk_size{
                    let mut new_read = vec![];
                    std::mem::swap(&mut new_read, &mut buffer);
                    rets.push(async_create_for_fn_id_0(ctx.spawn(), next_place, new_read));
                    next_place = (next_place + 1 ) % (world_size as Place) ;
                }
                buffer.push(s.into_bytes());
            }
        }
        rets.push(async_create_for_fn_id_0(ctx.spawn(), next_place, buffer));

        let mut global_table = CountTable::new();
        for ret in rets{
            let count_table = ret.await;
            merge_table(&mut global_table, count_table);
        }
        
        println!("got {} kmers", global_table.len());
        let p = global_table.iter().take(100).collect::<Vec<_>>();
        for (kmer, data) in p{
            println!("{}: {}", String::from_utf8_lossy(&kmer[..]), data.count);
        }

        let mut hist = vec![0usize;2048];
        for (_, data) in global_table{
            hist[data.count as usize] += 1;
        }
        println!("{:?}", hist);

        wait_all(ctx).await;
        info!("Main finished");
    }
}

pub fn main() {
    essence::genesis(inner_main())
}
