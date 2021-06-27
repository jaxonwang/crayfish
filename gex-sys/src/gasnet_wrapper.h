#define GASNET_SEQ
#include <gasnetex.h>
#include <gasnet_coll.h>
gex_Flags_t gex_flag_uses_gasnet1();
gex_Rank_t gex_rank_invalid();
gex_Event_t gex_event_invalid();
gex_Event_t gex_event_no_op();
gex_Event_t *gex_event_now();
gex_Event_t *gex_event_defer();
gex_Event_t *gex_event_group();
gex_Flags_t gex_flag_immediate();
gex_Flags_t gex_flag_ad_my_rank();
gex_Flags_t gex_flag_ad_my_nbrhd();
gex_Flags_t gex_flag_ad_favor_my_rank();
gex_Flags_t gex_flag_ad_favor_my_nbrhd();
gex_Flags_t gex_flag_ad_favor_remote();
gex_Flags_t gex_flag_ad_acq();
gex_Flags_t gex_flag_ad_rel();
gex_Flags_t gex_flag_rank_is_jobrank();
gex_Flags_t gex_flag_am_prepare_least_client();
gex_Flags_t gex_flag_am_prepare_least_alloc();
gex_Flags_t gex_flag_tm_global_scratch();
gex_Flags_t gex_flag_tm_local_scratch();
gex_Flags_t gex_flag_tm_symmetric_scratch();
gex_Flags_t gex_flag_tm_no_scratch();
gex_Flags_t gex_flag_globally_quiesced();
gex_EP_t gex_ep_invalid();
gex_Client_t gex_client_invalid();
gex_Segment_t gex_segment_invalid();
gex_TM_t gex_tm_invalid();
gex_TI_t gex_ti_srcrank();
gex_TI_t gex_ti_ep();
gex_TI_t gex_ti_entry();
gex_TI_t gex_ti_is_req();
gex_TI_t gex_ti_is_long();
gex_TI_t gex_ti_all();
gex_AM_SrcDesc_t gex_am_srcdesc_no_op();
gex_EC_t gex_ec_all();
gex_EC_t gex_ec_get();
gex_EC_t gex_ec_put();
gex_EC_t gex_ec_am();
gex_EC_t gex_ec_lc();
gex_EC_t gex_ec_rmw();
gex_Rank_t gex_System_QueryJobRank_Wrap(void);
gex_Rank_t gex_System_QueryJobSize_Wrap(void);
int gex_System_GetVerboseErrors_Wrap();
void gex_System_SetVerboseErrors_Wrap(int enable);
uint64_t gex_System_QueryMaxThreads_Wrap(void);
void gex_Client_SetCData_Wrap(gex_Client_t client, const void *val);
void *gex_Client_QueryCData_Wrap(gex_Client_t client);
void gex_Segment_SetCData_Wrap(gex_Segment_t seg, const void *val);
void *gex_Segment_QueryCData_Wrap(gex_Segment_t seg);
void gex_TM_SetCData_Wrap(gex_TM_t tm, const void *val);
void *gex_TM_QueryCData_Wrap(gex_TM_t tm);
void gex_EP_SetCData_Wrap(gex_EP_t ep, const void *val);
void *gex_EP_QueryCData_Wrap(gex_EP_t ep);
gex_Flags_t gex_Client_QueryFlags_Wrap(gex_Client_t client);
const char *gex_Client_QueryName_Wrap(gex_Client_t client);
int gex_Client_Init_Wrap(gex_Client_t *client_p, gex_EP_t *ep_p, gex_TM_t *tm_p,
                         const char *clientName, int *argc, char ***argv,
                         gex_Flags_t flags);
gex_Client_t gex_Segment_QueryClient_Wrap(gex_Segment_t seg);
gex_Flags_t gex_Segment_QueryFlags_Wrap(gex_Segment_t seg);
void *gex_Segment_QueryAddr_Wrap(gex_Segment_t seg);
uintptr_t gex_Segment_QuerySize_Wrap(gex_Segment_t seg);
int gex_Segment_Attach_Wrap(gex_Segment_t *segment_p, gex_TM_t tm,
                            uintptr_t size);
gex_Client_t gex_TM_QueryClient_Wrap(gex_TM_t tm);
gex_EP_t gex_TM_QueryEP_Wrap(gex_TM_t tm);
gex_Flags_t gex_TM_QueryFlags_Wrap(gex_TM_t tm);
gex_Rank_t gex_TM_QueryRank_Wrap(gex_TM_t tm);
gex_Rank_t gex_TM_QuerySize_Wrap(gex_TM_t tm);
size_t gex_TM_Split_Wrap(gex_TM_t *new_tm_p, gex_TM_t parent_tm, int color,
                         int key, void *scratch_addr, size_t scratch_size,
                         gex_Flags_t flags);
size_t gex_TM_Create_Wrap(gex_TM_t *new_tms, size_t num_new_tms,
                          gex_TM_t parent_tm, gex_EP_Location_t *args,
                          size_t numargs, gex_Addr_t *scratch_addrs,
                          size_t scratch_size, gex_Flags_t flags);
int gex_TM_Destroy_Wrap(gex_TM_t tm, gex_Memvec_t *scratch_p,
                        gex_Flags_t flags);
gex_TM_t gex_TM_Pair_Wrap(gex_EP_t local_ep, gex_EP_Index_t remote_ep_index);
gex_Rank_t gex_TM_TranslateRankToJobrank_Wrap(gex_TM_t tm, gex_Rank_t rank);
gex_Rank_t gex_TM_TranslateJobrankToRank_Wrap(gex_TM_t tm, gex_Rank_t jobrank);
gex_EP_Location_t gex_TM_TranslateRankToEP_Wrap(gex_TM_t tm, gex_Rank_t rank,
                                                gex_Flags_t flags);
gex_Client_t gex_EP_QueryClient_Wrap(gex_EP_t ep);
gex_Flags_t gex_EP_QueryFlags_Wrap(gex_EP_t ep);
gex_Segment_t gex_EP_QuerySegment_Wrap(gex_EP_t ep);
gex_EP_Index_t gex_EP_QueryIndex_Wrap(gex_EP_t ep);
gex_Event_t gex_EP_QueryBoundSegmentNB_Wrap(gex_TM_t tm, gex_Rank_t rank,
                                            void **owneraddr_p,
                                            void **localaddr_p,
                                            uintptr_t *size_p,
                                            gex_Flags_t flags);
int gex_Segment_QueryBound_Wrap(gex_TM_t tm, gex_Rank_t rank,
                                void **owneraddr_p, void **localaddr_p,
                                uintptr_t *size_p);
int gex_EP_PublishBoundSegment_Wrap(gex_TM_t tm, gex_EP_t *eps, size_t num_eps,
                                    gex_Flags_t flags);
int gex_EP_RegisterHandlers_Wrap(gex_EP_t ep, gex_AM_Entry_t *table,
                                 size_t numentries);
unsigned int gex_AM_MaxArgs_Wrap(void);
size_t gex_AM_MaxRequestLong_Wrap(gex_TM_t tm, gex_Rank_t other_rank,
                                  const gex_Event_t *lc_opt, gex_Flags_t flags,
                                  unsigned int numargs);
size_t gex_AM_MaxReplyLong_Wrap(gex_TM_t tm, gex_Rank_t other_rank,
                                const gex_Event_t *lc_opt, gex_Flags_t flags,
                                unsigned int numargs);
size_t gex_AM_MaxRequestMedium_Wrap(gex_TM_t tm, gex_Rank_t other_rank,
                                    const gex_Event_t *lc_opt,
                                    gex_Flags_t flags, unsigned int numargs);
size_t gex_AM_MaxReplyMedium_Wrap(gex_TM_t tm, gex_Rank_t other_rank,
                                  const gex_Event_t *lc_opt, gex_Flags_t flags,
                                  unsigned int numargs);
size_t gex_Token_MaxReplyLong_Wrap(gex_Token_t token, const gex_Event_t *lc_opt,
                                   gex_Flags_t flags, unsigned int numargs);
size_t gex_Token_MaxReplyMedium_Wrap(gex_Token_t token,
                                     const gex_Event_t *lc_opt,
                                     gex_Flags_t flags, unsigned int numargs);
size_t gex_AM_LUBRequestLong_Wrap(void);
size_t gex_AM_LUBReplyLong_Wrap(void);
size_t gex_AM_LUBRequestMedium_Wrap(void);
size_t gex_AM_LUBReplyMedium_Wrap(void);
gex_TI_t gex_Token_Info_Wrap(gex_Token_t token, gex_Token_Info_t *info,
                             gex_TI_t mask);
void *gex_AM_SrcDescAddr_Wrap(gex_AM_SrcDesc_t sd);
size_t gex_AM_SrcDescSize_Wrap(gex_AM_SrcDesc_t sd);
gex_AM_SrcDesc_t
gex_AM_PrepareRequestMedium_Wrap(gex_TM_t tm, gex_Rank_t rank,
                                 const void *client_buf, size_t least_payload,
                                 size_t most_payload, gex_Event_t *lc_opt,
                                 gex_Flags_t flags, unsigned int numargs);
gex_AM_SrcDesc_t
gex_AM_PrepareReplyMedium_Wrap(gex_Token_t token, const void *client_buf,
                               size_t least_payload, size_t most_payload,
                               gex_Event_t *lc_opt, gex_Flags_t flags,
                               unsigned int numargs);
gex_AM_SrcDesc_t gex_AM_PrepareRequestLong_Wrap(
    gex_TM_t tm, gex_Rank_t rank, const void *client_buf, size_t least_payload,
    size_t most_payload, void *dest_addr, gex_Event_t *lc_opt,
    gex_Flags_t flags, unsigned int numargs);
gex_AM_SrcDesc_t
gex_AM_PrepareReplyLong_Wrap(gex_Token_t token, const void *client_buf,
                             size_t least_payload, size_t most_payload,
                             void *dest_addr, gex_Event_t *lc_opt,
                             gex_Flags_t flags, unsigned int numargs);
int gex_RMA_PutBlocking_Wrap(gex_TM_t tm, gex_Rank_t rank, void *dest,
                             const void *src, size_t nbytes, gex_Flags_t flags);
int gex_RMA_PutNBI_Wrap(gex_TM_t tm, gex_Rank_t rank, void *dest,
                        const void *src, size_t nbytes, gex_Event_t *lc_opt,
                        gex_Flags_t flags);
gex_Event_t gex_RMA_PutNB_Wrap(gex_TM_t tm, gex_Rank_t rank, void *dest,
                               const void *src, size_t nbytes,
                               gex_Event_t *lc_opt, gex_Flags_t flags);
int gex_RMA_GetBlocking_Wrap(gex_TM_t tm, void *dest, gex_Rank_t rank,
                             void *src, size_t nbytes, gex_Flags_t flags);
int gex_RMA_GetNBI_Wrap(gex_TM_t tm, void *dest, gex_Rank_t rank, void *src,
                        size_t nbytes, gex_Flags_t flags);
gex_Event_t gex_RMA_GetNB_Wrap(gex_TM_t tm, void *dest, gex_Rank_t rank,
                               void *src, size_t nbytes, gex_Flags_t flags);
gex_RMA_Value_t gex_RMA_GetBlockingVal_Wrap(gex_TM_t tm, gex_Rank_t rank,
                                            void *src, size_t nbytes,
                                            gex_Flags_t flags);
int gex_RMA_PutBlockingVal_Wrap(gex_TM_t tm, gex_Rank_t rank, void *dest,
                                gex_RMA_Value_t value, size_t nbytes,
                                gex_Flags_t flags);
int gex_RMA_PutNBIVal_Wrap(gex_TM_t tm, gex_Rank_t rank, void *dest,
                           gex_RMA_Value_t value, size_t nbytes,
                           gex_Flags_t flags);
gex_Event_t gex_RMA_PutNBVal_Wrap(gex_TM_t tm, gex_Rank_t rank, void *dest,
                                  gex_RMA_Value_t value, size_t nbytes,
                                  gex_Flags_t flags);
void gex_NBI_BeginAccessRegion_Wrap(gex_Flags_t flags);
gex_Event_t gex_NBI_EndAccessRegion_Wrap(gex_Flags_t flags);
int gex_Event_Test_Wrap(gex_Event_t event);
void gex_Event_Wait_Wrap(gex_Event_t event);
int gex_Event_TestSome_Wrap(gex_Event_t *pevent, size_t numevents,
                            gex_Flags_t flags);
void gex_Event_WaitSome_Wrap(gex_Event_t *pevent, size_t numevents,
                             gex_Flags_t flags);
int gex_Event_TestAll_Wrap(gex_Event_t *pevent, size_t numevents,
                           gex_Flags_t flags);
void gex_Event_WaitAll_Wrap(gex_Event_t *pevent, size_t numevents,
                            gex_Flags_t flags);
int gex_NBI_Test_Wrap(gex_EC_t event_mask, gex_Flags_t flags);
void gex_NBI_Wait_Wrap(gex_EC_t event_mask, gex_Flags_t flags);
gex_Event_t gex_Event_QueryLeaf_Wrap(gex_Event_t root, gex_EC_t event_category);
void gex_System_QueryNbrhdInfo_Wrap(gex_RankInfo_t **info_p,
                                    gex_Rank_t *info_count_p,
                                    gex_Rank_t *my_info_index_p);
void gex_System_QueryHostInfo_Wrap(gex_RankInfo_t **info_p,
                                   gex_Rank_t *info_count_p,
                                   gex_Rank_t *my_info_index_p);
void gex_System_QueryMyPosition_Wrap(gex_Rank_t *nbrhd_set_size,
                                     gex_Rank_t *nbrhd_set_rank,
                                     gex_Rank_t *host_set_size,
                                     gex_Rank_t *host_set_rank);

uintptr_t gasnet_getMaxLocalSegmentSize_Wrap();
int gex_AM_RequestShort_Wrap0(gex_TM_t tm, gex_Rank_t rank,
                              gex_AM_Index_t handler, gex_Flags_t flags);
int gex_AM_RequestMedium_Wrap0(gex_TM_t tm, gex_Rank_t rank,
                               gex_AM_Index_t handler, const void *source_addr,
                               size_t nbytes, gex_Event_t *lc_opt,
                               gex_Flags_t flags);
int gex_AM_RequestLong_Wrap0(gex_TM_t tm, gex_Rank_t rank,
                             gex_AM_Index_t handler, const void *source_addr,
                             size_t nbytes, void *dest_addr,
                             gex_Event_t *lc_opt, gex_Flags_t flags);
int gex_AM_RequestShort_Wrap1(gex_TM_t tm, gex_Rank_t rank,
                              gex_AM_Index_t handler, gex_Flags_t flags,
                              gex_AM_Arg_t arg0);
int gex_AM_RequestMedium_Wrap1(gex_TM_t tm, gex_Rank_t rank,
                               gex_AM_Index_t handler, const void *source_addr,
                               size_t nbytes, gex_Event_t *lc_opt,
                               gex_Flags_t flags, gex_AM_Arg_t arg0);
int gex_AM_RequestLong_Wrap1(gex_TM_t tm, gex_Rank_t rank,
                             gex_AM_Index_t handler, const void *source_addr,
                             size_t nbytes, void *dest_addr,
                             gex_Event_t *lc_opt, gex_Flags_t flags,
                             gex_AM_Arg_t arg0);
int gex_AM_RequestShort_Wrap2(gex_TM_t tm, gex_Rank_t rank,
                              gex_AM_Index_t handler, gex_Flags_t flags,
                              gex_AM_Arg_t arg0, gex_AM_Arg_t arg1);
int gex_AM_RequestMedium_Wrap2(gex_TM_t tm, gex_Rank_t rank,
                               gex_AM_Index_t handler, const void *source_addr,
                               size_t nbytes, gex_Event_t *lc_opt,
                               gex_Flags_t flags, gex_AM_Arg_t arg0,
                               gex_AM_Arg_t arg1);
int gex_AM_RequestLong_Wrap2(gex_TM_t tm, gex_Rank_t rank,
                             gex_AM_Index_t handler, const void *source_addr,
                             size_t nbytes, void *dest_addr,
                             gex_Event_t *lc_opt, gex_Flags_t flags,
                             gex_AM_Arg_t arg0, gex_AM_Arg_t arg1);
int gex_AM_RequestShort_Wrap3(gex_TM_t tm, gex_Rank_t rank,
                              gex_AM_Index_t handler, gex_Flags_t flags,
                              gex_AM_Arg_t arg0, gex_AM_Arg_t arg1,
                              gex_AM_Arg_t arg2);
int gex_AM_RequestMedium_Wrap3(gex_TM_t tm, gex_Rank_t rank,
                               gex_AM_Index_t handler, const void *source_addr,
                               size_t nbytes, gex_Event_t *lc_opt,
                               gex_Flags_t flags, gex_AM_Arg_t arg0,
                               gex_AM_Arg_t arg1, gex_AM_Arg_t arg2);
int gex_AM_RequestLong_Wrap3(gex_TM_t tm, gex_Rank_t rank,
                             gex_AM_Index_t handler, const void *source_addr,
                             size_t nbytes, void *dest_addr,
                             gex_Event_t *lc_opt, gex_Flags_t flags,
                             gex_AM_Arg_t arg0, gex_AM_Arg_t arg1,
                             gex_AM_Arg_t arg2);
int gex_AM_RequestShort_Wrap4(gex_TM_t tm, gex_Rank_t rank,
                              gex_AM_Index_t handler, gex_Flags_t flags,
                              gex_AM_Arg_t arg0, gex_AM_Arg_t arg1,
                              gex_AM_Arg_t arg2, gex_AM_Arg_t arg3);
int gex_AM_RequestMedium_Wrap4(gex_TM_t tm, gex_Rank_t rank,
                               gex_AM_Index_t handler, const void *source_addr,
                               size_t nbytes, gex_Event_t *lc_opt,
                               gex_Flags_t flags, gex_AM_Arg_t arg0,
                               gex_AM_Arg_t arg1, gex_AM_Arg_t arg2,
                               gex_AM_Arg_t arg3);
int gex_AM_RequestLong_Wrap4(gex_TM_t tm, gex_Rank_t rank,
                             gex_AM_Index_t handler, const void *source_addr,
                             size_t nbytes, void *dest_addr,
                             gex_Event_t *lc_opt, gex_Flags_t flags,
                             gex_AM_Arg_t arg0, gex_AM_Arg_t arg1,
                             gex_AM_Arg_t arg2, gex_AM_Arg_t arg3);
int gex_AM_RequestLong_Wrap6(gex_TM_t tm, gex_Rank_t rank,
                             gex_AM_Index_t handler, const void *source_addr,
                             size_t nbytes, void *dest_addr,
                             gex_Event_t *lc_opt, gex_Flags_t flags,
                             gex_AM_Arg_t arg0, gex_AM_Arg_t arg1,
                             gex_AM_Arg_t arg2, gex_AM_Arg_t arg3,
                             gex_AM_Arg_t arg4, gex_AM_Arg_t arg5);
int gex_AM_RequestLong_Wrap7(gex_TM_t tm, gex_Rank_t rank,
                             gex_AM_Index_t handler, const void *source_addr,
                             size_t nbytes, void *dest_addr,
                             gex_Event_t *lc_opt, gex_Flags_t flags,
                             gex_AM_Arg_t arg0, gex_AM_Arg_t arg1,
                             gex_AM_Arg_t arg2, gex_AM_Arg_t arg3,
                             gex_AM_Arg_t arg4, gex_AM_Arg_t arg5, gex_AM_Arg_t arg6);
int gex_AM_ReplyShort_Wrap0(gex_Token_t token, gex_AM_Index_t handler,
                            gex_Flags_t flags);
int gex_AM_ReplyMedium_Wrap0(gex_Token_t token, gex_AM_Index_t handler,
                             const void *source_addr, size_t nbytes,
                             gex_Event_t *lc_opt, gex_Flags_t flags);
int gex_AM_ReplyLong_Wrap0(gex_Token_t token, gex_AM_Index_t handler,
                             const void *source_addr, size_t nbytes,
                             void *dest_addr,
                             gex_Event_t *lc_opt, gex_Flags_t flags);

// collective
gex_Event_t gex_Coll_BarrierNB_Wrap(gex_TM_t tm, gex_Flags_t flags);
gex_Event_t gex_Coll_BroadcastNB_Wrap(gex_TM_t tm, gex_Rank_t root, void *dst,
                                      const void *src, size_t nbytes,
                                      gex_Flags_t flags);
int gasnet_AMPoll_Wrap ();
