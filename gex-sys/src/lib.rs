#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(improper_ctypes)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]
#![allow(unaligned_references)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use std::convert::TryInto;
use std::default::Default;
use std::env::args;
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::os::raw::*;
use std::ptr::null;
use std::ptr::null_mut;

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
    for c_arg in &mut c_args {
        let pos = c_arg as *mut *mut c_char;
        if ptr <= pos && pos < unsafe { ptr.offset(argc as isize) } {
            ret.push(unsafe { CString::from_raw(*c_arg) }.into_string().unwrap());
        } else {
            drop(unsafe { CString::from_raw(*c_arg) });
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

impl<I: std::slice::SliceIndex<[gex_AM_Entry_t]>> std::ops::Index<I> for Entrytable {
    type Output = I::Output;
    fn index(&self, index: I) -> &Self::Output {
        &self.entries[index]
    }
}

impl Default for Entrytable {
    fn default() -> Self {
        Self::new()
    }
}

impl Entrytable {
    pub fn new() -> Self {
        Entrytable { entries: vec![] }
    }

    pub unsafe fn add(
        &mut self,
        index: gex_AM_Index_t,
        f: *const (),
        flags: gex_Flags_t,
        nargs: usize,
        name: Option<&'static str>,
    ) {
        let handler = std::mem::transmute::<*const (), extern "C" fn()>(f);
        let entry = gex_AM_Entry_t {
            gex_index: index,
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

    pub unsafe fn add_short_req(
        &mut self,
        index: gex_AM_Index_t,
        f: *const (),
        nargs: usize,
        name: Option<&'static str>,
    ) {
        self.add(
            index,
            f,
            GEX_FLAG_AM_SHORT | GEX_FLAG_AM_REQUEST,
            nargs,
            name,
        );
    }

    pub unsafe fn add_medium_req(
        &mut self,
        index: gex_AM_Index_t,
        f: *const (),
        nargs: usize,
        name: Option<&'static str>,
    ) {
        self.add(
            index,
            f,
            GEX_FLAG_AM_MEDIUM | GEX_FLAG_AM_REQUEST,
            nargs,
            name,
        );
    }

    pub unsafe fn add_long_req(
        &mut self,
        index: gex_AM_Index_t,
        f: *const (),
        nargs: usize,
        name: Option<&'static str>,
    ) {
        self.add(
            index,
            f,
            GEX_FLAG_AM_LONG | GEX_FLAG_AM_REQUEST,
            nargs,
            name,
        );
    }

    pub unsafe fn add_short_reply(
        &mut self,
        index: gex_AM_Index_t,
        f: *const (),
        nargs: usize,
        name: Option<&'static str>,
    ) {
        self.add(index, f, GEX_FLAG_AM_SHORT | GEX_FLAG_AM_REPLY, nargs, name);
    }

    pub unsafe fn add_medium_reply(
        &mut self,
        index: gex_AM_Index_t,
        f: *const (),
        nargs: usize,
        name: Option<&'static str>,
    ) {
        self.add(
            index,
            f,
            GEX_FLAG_AM_MEDIUM | GEX_FLAG_AM_REPLY,
            nargs,
            name,
        );
    }

    pub unsafe fn add_long_reply(
        &mut self,
        index: gex_AM_Index_t,
        f: *const (),
        nargs: usize,
        name: Option<&'static str>,
    ) {
        self.add(index, f, GEX_FLAG_AM_LONG | GEX_FLAG_AM_REPLY, nargs, name);
    }
}

pub fn gex_register_entries(ep: gex_EP_t, entries: &mut Entrytable) {
    let et = &mut entries.entries;
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

pub fn gax_segment_query_addr(seg: gex_Segment_t) -> *const c_void {
    unsafe { gex_Segment_QueryAddr_Wrap(seg) }
}

pub fn gax_segment_query_size(seg: gex_Segment_t) -> usize {
    unsafe { gex_Segment_QuerySize_Wrap(seg) }
}

pub fn gex_event_wait(event: gex_Event_t) {
    unsafe { gex_Event_Wait_Wrap(event) };
}

/// return true if event success
pub fn gex_event_done(event: gex_Event_t) -> bool {
    unsafe { gex_Event_Test_Wrap(event) == 0 }
}

pub fn gex_ep_query_bound_segment(tm: gex_TM_t, rank: gex_Rank_t) -> (*mut c_void, usize) {
    let mut dest_addr = uninit::<*mut c_void>();
    let mut size = uninit::<usize>();
    let event = unsafe {
        gex_EP_QueryBoundSegmentNB_Wrap(
            tm,
            rank,
            dest_addr.as_mut_ptr(),
            null_mut::<*mut c_void>(),
            size.as_mut_ptr(),
            0,
        )
    };
    let dest_addr = unsafe { dest_addr.assume_init() };
    let size = unsafe { size.assume_init() };
    gex_event_wait(event);
    assert! {size!=0, "size is 0"};
    (dest_addr, size)
}

const MAX_ARGS_USED: u32 = 4;

pub fn gex_am_max_global_request_long(tm: gex_TM_t) -> usize {
    let size;
    unsafe {
        let rank = gex_rank_invalid(); // use invalid rank to query global
        size = gex_AM_MaxRequestLong_Wrap(tm, rank, gex_event_now(), 0, MAX_ARGS_USED);
    }
    assert! {size!=0, "size is 0"};
    size.try_into().unwrap()
}

pub fn gex_am_max_request_long(tm: gex_TM_t, rank: gex_Rank_t) -> usize {
    let size;
    unsafe {
        size = gex_AM_MaxRequestLong_Wrap(tm, rank, gex_event_now(), 0, MAX_ARGS_USED);
    }
    assert! {size!=0, "size is 0"};
    size.try_into().unwrap()
}

pub fn gex_am_max_global_request_medium(tm: gex_TM_t) -> usize {
    let size;
    unsafe {
        let rank = gex_rank_invalid(); // use invalid rank to query global
        size = gex_AM_MaxRequestMedium_Wrap(tm, rank, gex_event_now(), 0, MAX_ARGS_USED);
    }
    assert! {size!=0, "size is 0"};
    size.try_into().unwrap()
}

pub fn gex_am_max_request_medium(tm: gex_TM_t, rank: gex_Rank_t) -> usize {
    let size;
    unsafe {
        size = gex_AM_MaxRequestMedium_Wrap(tm, rank, gex_event_now(), 0, MAX_ARGS_USED);
    }
    assert! {size!=0, "size is 0"};
    size.try_into().unwrap()
}

pub fn gex_am_reqeust_short0(tm: gex_TM_t, rank: gex_Rank_t, handler: gex_AM_Index_t) {
    unsafe {
        assert_gasnet_ok(gex_AM_RequestShort_Wrap0(tm, rank, handler, 0));
    }
}

pub fn gex_am_reqeust_short1(
    tm: gex_TM_t,
    rank: gex_Rank_t,
    handler: gex_AM_Index_t,
    arg0: gex_AM_Arg_t,
) {
    unsafe {
        assert_gasnet_ok(gex_AM_RequestShort_Wrap1(tm, rank, handler, 0, arg0));
    }
}

pub unsafe fn gex_am_reqeust_medium0(
    tm: gex_TM_t,
    rank: gex_Rank_t,
    handler: gex_AM_Index_t,
    source_addr: *const ::std::os::raw::c_void,
    nbytes: size_t,
    lc_opt: *mut gex_Event_t,
) {
    assert_gasnet_ok(gex_AM_RequestMedium_Wrap0(
        tm,
        rank,
        handler,
        source_addr,
        nbytes,
        lc_opt,
        0,
    ));
}

pub unsafe fn gex_am_reqeust_long0(
    tm: gex_TM_t,
    rank: gex_Rank_t,
    handler: gex_AM_Index_t,
    source_addr: *const ::std::os::raw::c_void,
    nbytes: size_t,
    dest_addr: *mut ::std::os::raw::c_void,
    dest_offset: isize,
    lc_opt: *mut gex_Event_t,
) {
    assert_gasnet_ok(gex_AM_RequestLong_Wrap0(
        tm,
        rank,
        handler,
        source_addr,
        nbytes,
        dest_addr.offset(dest_offset),
        lc_opt,
        0,
    ));
}

pub unsafe fn gex_am_reqeust_long4(
    tm: gex_TM_t,
    rank: gex_Rank_t,
    handler: gex_AM_Index_t,
    source_addr: *const ::std::os::raw::c_void,
    nbytes: size_t,
    dest_addr: *mut ::std::os::raw::c_void,
    dest_offset: isize,
    lc_opt: *mut gex_Event_t,
    arg0: gasnet_handlerarg_t,
    arg1: gasnet_handlerarg_t,
    arg2: gasnet_handlerarg_t,
    arg3: gasnet_handlerarg_t,
) {
    assert_gasnet_ok(gex_AM_RequestLong_Wrap4(
        tm,
        rank,
        handler,
        source_addr,
        nbytes,
        dest_addr.offset(dest_offset),
        lc_opt,
        0,
        arg0,
        arg1,
        arg2,
        arg3,
    ));
}

pub unsafe fn gex_am_reqeust_long6(
    tm: gex_TM_t,
    rank: gex_Rank_t,
    handler: gex_AM_Index_t,
    source_addr: *const ::std::os::raw::c_void,
    nbytes: size_t,
    dest_addr: *mut ::std::os::raw::c_void,
    dest_offset: isize,
    lc_opt: *mut gex_Event_t,
    arg0: gasnet_handlerarg_t,
    arg1: gasnet_handlerarg_t,
    arg2: gasnet_handlerarg_t,
    arg3: gasnet_handlerarg_t,
    arg4: gasnet_handlerarg_t,
    arg5: gasnet_handlerarg_t,
) {
    assert_gasnet_ok(gex_AM_RequestLong_Wrap6(
        tm,
        rank,
        handler,
        source_addr,
        nbytes,
        dest_addr.offset(dest_offset),
        lc_opt,
        0,
        arg0,
        arg1,
        arg2,
        arg3,
        arg4,
        arg5,
    ));
}

pub fn gex_am_reply_short0(token: gex_Token_t, handler: gex_AM_Index_t) {
    unsafe {
        assert_gasnet_ok(gex_AM_ReplyShort_Wrap0(token, handler, 0));
    }
}

pub unsafe fn gex_am_reply_medium0(
    token: gex_Token_t,
    handler: gex_AM_Index_t,
    source_addr: *const ::std::os::raw::c_void,
    nbytes: size_t,
    lc_opt: *mut gex_Event_t,
) {
    assert_gasnet_ok(gex_AM_ReplyMedium_Wrap0(
        token,
        handler,
        source_addr,
        nbytes,
        lc_opt,
        0,
    ));
}

pub unsafe fn gex_am_reply_long0(
    token: gex_Token_t,
    handler: gex_AM_Index_t,
    source_addr: *const ::std::os::raw::c_void,
    nbytes: size_t,
    dest_addr: *mut ::std::os::raw::c_void,
    lc_opt: *mut gex_Event_t,
) {
    assert_gasnet_ok(gex_AM_ReplyLong_Wrap0(
        token,
        handler,
        source_addr,
        nbytes,
        dest_addr,
        lc_opt,
        0,
    ));
}

pub fn gex_nbi_wait_am_lc() {
    // wait am local complete
    unsafe {
        gex_NBI_Wait_Wrap(gex_ec_am(), 0);
    }
}

pub fn gex_coll_barrier_nb(tm: gex_TM_t) -> gex_Event_t {
    unsafe { gex_Coll_BarrierNB_Wrap(tm, 0) }
}

pub unsafe fn gex_coll_broadcast_nb(
    tm: gex_TM_t,
    root: gex_Rank_t,
    dst: *mut ::std::os::raw::c_void,
    src: *const ::std::os::raw::c_void,
    nbytes: size_t,
) -> gex_Event_t {
    gex_Coll_BroadcastNB_Wrap(tm, root, dst, src, nbytes, 0)
}

pub unsafe fn gex_rma_putblocking(
    tm: gex_TM_t,
    rank: gex_Rank_t,
    dest_addr: *mut ::std::os::raw::c_void,
    source_addr: *const ::std::os::raw::c_void,
    nbytes: size_t,
) {
    gex_RMA_PutBlocking_Wrap(tm, rank, dest_addr, source_addr, nbytes, 0);
}

pub fn gasnet_ampoll() {
    unsafe {
        gasnet_AMPoll_Wrap();
    }
}
