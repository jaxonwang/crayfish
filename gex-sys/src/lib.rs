#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(improper_ctypes)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use std::os::raw::*;
use std::env::args;
use std::default::Default;
use std::ptr::null;
use std::mem::MaybeUninit;
use std::ffi::CString;

// #[link(name = "gasnet_wrapper", kind = "static")]
pub fn gex_Client_init() -> (Vec<String>, gex_Client_t, gex_EP_t, gex_TM_t){
    let args = args().map(|arg| CString::new(arg).unwrap() ).collect::<Vec<CString>>();
    let mut c_args = args.into_iter().map(|arg| arg.into_raw()).collect::<Vec<*mut c_char>>();
    let mut argc = c_args.len() as c_int;
    let mut ptr :*mut *mut c_char = c_args.as_mut_ptr();
    let mut client = MaybeUninit::<gex_Client_t>::uninit(); 
    let mut ep = MaybeUninit::<gex_EP_t>::uninit(); 
    let mut tm = MaybeUninit::<gex_TM_t>::uninit(); 
    unsafe{
            gex_Client_Init_Wrap(
            client.as_mut_ptr(),
            ep.as_mut_ptr(),
            tm.as_mut_ptr(),
            null::<c_char>(),
            &mut argc as *mut c_int,
            &mut ptr as *mut *mut *mut c_char,
            0,
            );
    }
    let client = unsafe {client.assume_init()};
    let ep = unsafe {ep.assume_init()};
    let tm = unsafe {tm.assume_init()};
    let mut ret = vec![];
    for i in 0..c_args.len(){
        let pos = &mut c_args[i] as *mut *mut c_char;
        if ptr <= pos && pos < unsafe{ ptr.offset(argc as isize)} {
            ret.push(unsafe{CString::from_raw(c_args[i])}.into_string().unwrap());
        }else{
            drop(unsafe{CString::from_raw(c_args[i])});
        }
    }
    (ret, client, ep, tm)
}

pub fn gex_Segment_QueryAddr(seg: gex_Segment_t) -> *const u8{
    unsafe{
        gex_Segment_QueryAddr_Wrap(seg) as *const u8
    }
}

pub fn gex_System_QueryJobRank() -> gex_Rank_t{
    unsafe{
        gex_System_QueryJobRank_Wrap()
    }
} 

pub fn gex_System_QueryJobSize() -> gex_Rank_t{
    unsafe{
        gex_System_QueryJobSize_Wrap()
    }
} 
