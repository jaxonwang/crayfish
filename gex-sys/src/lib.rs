#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(improper_ctypes)]
#![allow(safe_packed_borrows)] // TODO: remove this when bindgen fix it

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use std::default::Default;
use std::env::args;
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::os::raw::*;
use std::ptr::null;

fn uninit<T>() -> MaybeUninit<T> {
    MaybeUninit::<T>::uninit()
}

pub fn assert_gasnet_ok(ret: c_int) {
    assert_eq!(ret, GASNET_OK as c_int, "GASNet: {}", unsafe {
        CStr::from_ptr(gasnet_ErrorName(ret) as *const c_char)
            .to_str()
            .unwrap()
    });
}

fn literal_to_pointer(literal: &'static [u8]) -> *const c_char {
    CStr::from_bytes_with_nul(literal).unwrap().as_ptr()
}

pub fn gex_client_init() -> (Vec<String>, gex_Client_t, gex_EP_t, gex_TM_t) {
    let args = args()
        .map(|arg| CString::new(arg).unwrap())
        .collect::<Vec<CString>>();
    let mut c_args = args
        .into_iter()
        .map(|arg| arg.into_raw())
        .collect::<Vec<*mut c_char>>();
    let mut argc = c_args.len() as c_int;
    let mut ptr: *mut *mut c_char = c_args.as_mut_ptr();
    let mut client = uninit::<gex_Client_t>();
    let mut ep = uninit::<gex_EP_t>();
    let mut tm = uninit::<gex_TM_t>();
    unsafe {
        assert_gasnet_ok(gex_Client_Init_Wrap(
            client.as_mut_ptr(),
            ep.as_mut_ptr(),
            tm.as_mut_ptr(),
            literal_to_pointer(b"rust-apgas\0"),
            &mut argc as *mut c_int,
            &mut ptr as *mut *mut *mut c_char,
            0,
        ));
    }
    let client = unsafe { client.assume_init() };
    let ep = unsafe { ep.assume_init() };
    let tm = unsafe { tm.assume_init() };
    let mut ret = vec![];
    for i in 0..c_args.len() {
        let pos = &mut c_args[i] as *mut *mut c_char;
        if ptr <= pos && pos < unsafe { ptr.offset(argc as isize) } {
            ret.push(
                unsafe { CString::from_raw(c_args[i]) }
                    .into_string()
                    .unwrap(),
            );
        } else {
            drop(unsafe { CString::from_raw(c_args[i]) });
        }
    }
    (ret, client, ep, tm)
}

pub fn gex_token_info(token: gex_Token_t) -> gex_Token_Info_t {
    let mut token_info = uninit::<gex_Token_Info_t>();
    unsafe {
        gex_Token_Info_Wrap(
            token,
            token_info.as_mut_ptr(),
            gex_ti_srcrank() | gex_ti_ep(),
        ); // return bitmap indicating the supported field. ignore.
        token_info.assume_init()
    }
}

pub fn gex_segment_attach(tm: gex_TM_t, length: usize) -> gex_Segment_t {
    let mut seg = uninit::<gex_Segment_t>();
    unsafe {
        assert_gasnet_ok(gex_Segment_Attach_Wrap(seg.as_mut_ptr(), tm, length));
        seg.assume_init()
    }
}

pub struct Entrytable {
    entries: Vec<gex_AM_Entry_t>,
}

impl Entrytable {
    pub fn new() -> Self {
        Entrytable { entries: vec![] }
    }

    pub fn add(
        &mut self,
        f: *const (),
        flags: gex_Flags_t,
        nargs: usize,
        name: Option<&'static str>,
    ) {
        let handler = unsafe { std::mem::transmute::<*const (), extern "C" fn()>(f) };
        let entry = gex_AM_Entry_t {
            gex_index: 0,
            gex_fnptr: Some(handler),
            gex_flags: flags,
            gex_nargs: nargs as c_uint,
            gex_cdata: null::<c_void>(),
            gex_name: match name {
                None => null::<c_char>(),
                Some(s) => s.as_ptr() as *const c_char,
            },
        };
        self.entries.push(entry);
    }

    pub fn add_short_req(&mut self, f: *const (), nargs: usize, name: Option<&'static str>) {
        self.add(f, GEX_FLAG_AM_SHORT | GEX_FLAG_AM_REQUEST, nargs, name);
    }

    pub fn add_medium_req(&mut self, f: *const (), nargs: usize, name: Option<&'static str>) {
        self.add(f, GEX_FLAG_AM_MEDIUM | GEX_FLAG_AM_REQUEST, nargs, name);
    }

    pub fn add_long_req(&mut self, f: *const (), nargs: usize, name: Option<&'static str>) {
        self.add(f, GEX_FLAG_AM_LONG | GEX_FLAG_AM_REQUEST, nargs, name);
    }
}

pub fn gex_register_entries(ep: gex_EP_t, entries: &mut Entrytable) {
    let ref mut et = entries.entries;
    assert_gasnet_ok(unsafe { gex_EP_RegisterHandlers_Wrap(ep, et.as_mut_ptr(), et.len() as u64) });
}

pub fn gasnet_get_max_local_segment_size() -> usize {
    unsafe { gasnet_getMaxLocalSegmentSize_Wrap() }
}

pub fn gax_system_query_jobrank() -> gex_Rank_t {
    unsafe { gex_System_QueryJobRank_Wrap() }
}

pub fn gax_system_query_jobsize() -> gex_Rank_t {
    unsafe { gex_System_QueryJobSize_Wrap() }
}
