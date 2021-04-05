#include <gasnet_wrapper.h>
extern gex_Flags_t gex_flag_uses_gasnet1() { return GEX_FLAG_USES_GASNET1; }
extern gex_Rank_t gex_rank_invalid() { return GEX_RANK_INVALID; }
extern gex_Event_t gex_event_invalid() { return GEX_EVENT_INVALID; }
extern gex_Event_t gex_event_no_op() { return GEX_EVENT_NO_OP; }
extern gex_Event_t* gex_event_now() { return GEX_EVENT_NOW; }
extern gex_Event_t* gex_event_defer() { return GEX_EVENT_DEFER; }
extern gex_Event_t* gex_event_group() { return GEX_EVENT_GROUP; }
extern gex_Flags_t gex_flag_immediate() { return GEX_FLAG_IMMEDIATE; }
extern gex_Flags_t gex_flag_ad_my_rank() { return GEX_FLAG_AD_MY_RANK; }
extern gex_Flags_t gex_flag_ad_my_nbrhd() { return GEX_FLAG_AD_MY_NBRHD; }
extern gex_Flags_t gex_flag_ad_favor_my_rank() { return GEX_FLAG_AD_FAVOR_MY_RANK; }
extern gex_Flags_t gex_flag_ad_favor_my_nbrhd() { return GEX_FLAG_AD_FAVOR_MY_NBRHD; }
extern gex_Flags_t gex_flag_ad_favor_remote() { return GEX_FLAG_AD_FAVOR_REMOTE; }
extern gex_Flags_t gex_flag_ad_acq() { return GEX_FLAG_AD_ACQ; }
extern gex_Flags_t gex_flag_ad_rel() { return GEX_FLAG_AD_REL; }
extern gex_Flags_t gex_flag_rank_is_jobrank() { return GEX_FLAG_RANK_IS_JOBRANK; }
extern gex_Flags_t gex_flag_am_prepare_least_client() { return GEX_FLAG_AM_PREPARE_LEAST_CLIENT; }
extern gex_Flags_t gex_flag_am_prepare_least_alloc() { return GEX_FLAG_AM_PREPARE_LEAST_ALLOC; }
extern gex_Flags_t gex_flag_tm_global_scratch() { return GEX_FLAG_TM_GLOBAL_SCRATCH; }
extern gex_Flags_t gex_flag_tm_local_scratch() { return GEX_FLAG_TM_LOCAL_SCRATCH; }
extern gex_Flags_t gex_flag_tm_symmetric_scratch() { return GEX_FLAG_TM_SYMMETRIC_SCRATCH; }
extern gex_Flags_t gex_flag_tm_no_scratch() { return GEX_FLAG_TM_NO_SCRATCH; }
extern gex_Flags_t gex_flag_globally_quiesced() { return GEX_FLAG_GLOBALLY_QUIESCED; }
extern gex_EP_t gex_ep_invalid() { return GEX_EP_INVALID; }
extern gex_Client_t gex_client_invalid() { return GEX_CLIENT_INVALID; }
extern gex_Segment_t gex_segment_invalid() { return GEX_SEGMENT_INVALID; }
extern gex_TM_t gex_tm_invalid() { return GEX_TM_INVALID; }
extern gex_TI_t gex_ti_srcrank() { return GEX_TI_SRCRANK; }
extern gex_TI_t gex_ti_ep() { return GEX_TI_EP; }
extern gex_TI_t gex_ti_entry() { return GEX_TI_ENTRY; }
extern gex_TI_t gex_ti_is_req() { return GEX_TI_IS_REQ; }
extern gex_TI_t gex_ti_is_long() { return GEX_TI_IS_LONG; }
extern gex_TI_t gex_ti_all() { return GEX_TI_ALL; }
extern gex_AM_SrcDesc_t gex_am_srcdesc_no_op() { return GEX_AM_SRCDESC_NO_OP; }
extern gex_EC_t gex_ec_all() { return GEX_EC_ALL; }
extern gex_EC_t gex_ec_get() { return GEX_EC_GET; }
extern gex_EC_t gex_ec_put() { return GEX_EC_PUT; }
extern gex_EC_t gex_ec_am() { return GEX_EC_AM; }
extern gex_EC_t gex_ec_lc() { return GEX_EC_LC; }
extern gex_EC_t gex_ec_rmw() { return GEX_EC_RMW; }
extern void gasnet_QueryGexObjects_Wrap(gex_Client_t *client_p,gex_EP_t *endpoint_p,gex_TM_t *tm_p,gex_Segment_t *segment_p){ gasnet_QueryGexObjects(client_p,endpoint_p,tm_p,segment_p); }
extern gex_Rank_t gex_System_QueryJobRank_Wrap(void){ return gex_System_QueryJobRank(); }
extern gex_Rank_t gex_System_QueryJobSize_Wrap(void){ return gex_System_QueryJobSize(); }
extern int gex_System_GetVerboseErrors_Wrap(){ return gex_System_GetVerboseErrors(); }
extern void gex_System_SetVerboseErrors_Wrap(int enable){ gex_System_SetVerboseErrors(enable); }
extern uint64_t gex_System_QueryMaxThreads_Wrap(void){ return gex_System_QueryMaxThreads(); }
extern void gex_Client_SetCData_Wrap(gex_Client_t client, const void *val){ gex_Client_SetCData(client,val); }
extern void* gex_Client_QueryCData_Wrap(gex_Client_t client){ return gex_Client_QueryCData(client); }
extern void gex_Segment_SetCData_Wrap(gex_Segment_t seg, const void *val){ gex_Segment_SetCData(seg,val); }
extern void* gex_Segment_QueryCData_Wrap(gex_Segment_t seg){ return gex_Segment_QueryCData(seg); }
extern void gex_TM_SetCData_Wrap(gex_TM_t tm, const void *val){ gex_TM_SetCData(tm,val); }
extern void* gex_TM_QueryCData_Wrap(gex_TM_t tm){ return gex_TM_QueryCData(tm); }
extern void gex_EP_SetCData_Wrap(gex_EP_t ep, const void *val){ gex_EP_SetCData(ep,val); }
extern void* gex_EP_QueryCData_Wrap(gex_EP_t ep){ return gex_EP_QueryCData(ep); }
extern gex_Flags_t gex_Client_QueryFlags_Wrap(gex_Client_t client){ return gex_Client_QueryFlags(client); }
extern const char * gex_Client_QueryName_Wrap(gex_Client_t client){ return gex_Client_QueryName(client); }
extern int gex_Client_Init_Wrap(gex_Client_t *client_p,gex_EP_t *ep_p,gex_TM_t *tm_p,const char *clientName,int *argc,char ***argv,gex_Flags_t flags){ return gex_Client_Init(client_p,ep_p,tm_p,clientName,argc,argv,flags); }
extern gex_Client_t gex_Segment_QueryClient_Wrap(gex_Segment_t seg){ return gex_Segment_QueryClient(seg); }
extern gex_Flags_t gex_Segment_QueryFlags_Wrap(gex_Segment_t seg){ return gex_Segment_QueryFlags(seg); }
extern void * gex_Segment_QueryAddr_Wrap(gex_Segment_t seg){ return gex_Segment_QueryAddr(seg); }
extern uintptr_t gex_Segment_QuerySize_Wrap(gex_Segment_t seg){ return gex_Segment_QuerySize(seg); }
extern int gex_Segment_Attach_Wrap(gex_Segment_t *segment_p,gex_TM_t tm,uintptr_t size){ return gex_Segment_Attach(segment_p,tm,size); }
extern gex_Client_t gex_TM_QueryClient_Wrap(gex_TM_t tm){ return gex_TM_QueryClient(tm); }
extern gex_EP_t gex_TM_QueryEP_Wrap(gex_TM_t tm){ return gex_TM_QueryEP(tm); }
extern gex_Flags_t gex_TM_QueryFlags_Wrap(gex_TM_t tm){ return gex_TM_QueryFlags(tm); }
extern gex_Rank_t gex_TM_QueryRank_Wrap(gex_TM_t tm){ return gex_TM_QueryRank(tm); }
extern gex_Rank_t gex_TM_QuerySize_Wrap(gex_TM_t tm){ return gex_TM_QuerySize(tm); }
extern size_t gex_TM_Split_Wrap(gex_TM_t *new_tm_p, gex_TM_t parent_tm, int color, int key,void *scratch_addr, size_t scratch_size,gex_Flags_t flags){ return gex_TM_Split(new_tm_p,parent_tm,color,key,scratch_addr,scratch_size,flags); }
extern size_t gex_TM_Create_Wrap(gex_TM_t *new_tms,size_t num_new_tms,gex_TM_t parent_tm,gex_EP_Location_t *args,size_t numargs,gex_Addr_t *scratch_addrs,size_t scratch_size,gex_Flags_t flags){ return gex_TM_Create(new_tms,num_new_tms,parent_tm,args,numargs,scratch_addrs,scratch_size,flags); }
extern int gex_TM_Destroy_Wrap(gex_TM_t tm,gex_Memvec_t *scratch_p,gex_Flags_t flags){ return gex_TM_Destroy(tm,scratch_p,flags); }
extern gex_TM_t gex_TM_Pair_Wrap(gex_EP_t local_ep,gex_EP_Index_t remote_ep_index){ return gex_TM_Pair(local_ep,remote_ep_index); }
extern gex_Rank_t gex_TM_TranslateRankToJobrank_Wrap(gex_TM_t tm, gex_Rank_t rank){ return gex_TM_TranslateRankToJobrank(tm,rank); }
extern gex_Rank_t gex_TM_TranslateJobrankToRank_Wrap(gex_TM_t tm, gex_Rank_t jobrank){ return gex_TM_TranslateJobrankToRank(tm,jobrank); }
extern gex_EP_Location_t gex_TM_TranslateRankToEP_Wrap(gex_TM_t tm,gex_Rank_t rank,gex_Flags_t flags){ return gex_TM_TranslateRankToEP(tm,rank,flags); }
extern gex_Client_t gex_EP_QueryClient_Wrap(gex_EP_t ep){ return gex_EP_QueryClient(ep); }
extern gex_Flags_t gex_EP_QueryFlags_Wrap(gex_EP_t ep){ return gex_EP_QueryFlags(ep); }
extern gex_Segment_t gex_EP_QuerySegment_Wrap(gex_EP_t ep){ return gex_EP_QuerySegment(ep); }
extern gex_EP_Index_t gex_EP_QueryIndex_Wrap(gex_EP_t ep){ return gex_EP_QueryIndex(ep); }
extern gex_Event_t gex_EP_QueryBoundSegmentNB_Wrap(gex_TM_t tm,gex_Rank_t rank,void **owneraddr_p,void **localaddr_p,uintptr_t *size_p,gex_Flags_t flags){ return gex_EP_QueryBoundSegmentNB(tm,rank,owneraddr_p,localaddr_p,size_p,flags); }
extern int gex_Segment_QueryBound_Wrap(gex_TM_t tm,gex_Rank_t rank,void **owneraddr_p,void **localaddr_p,uintptr_t *size_p){ return gex_Segment_QueryBound(tm,rank,owneraddr_p,localaddr_p,size_p); }
extern int gex_EP_PublishBoundSegment_Wrap(gex_TM_t tm,gex_EP_t *eps,size_t num_eps,gex_Flags_t flags){ return gex_EP_PublishBoundSegment(tm,eps,num_eps,flags); }
extern int gex_EP_RegisterHandlers_Wrap(gex_EP_t ep,gex_AM_Entry_t *table,size_t numentries){ return gex_EP_RegisterHandlers(ep,table,numentries); }
extern unsigned int gex_AM_MaxArgs_Wrap(void){ return gex_AM_MaxArgs(); }
extern size_t gex_AM_MaxRequestLong_Wrap(gex_TM_t tm,gex_Rank_t other_rank,const gex_Event_t *lc_opt,gex_Flags_t flags,unsigned int numargs){ return gex_AM_MaxRequestLong(tm,other_rank,lc_opt,flags,numargs); }
extern size_t gex_AM_MaxReplyLong_Wrap(gex_TM_t tm,gex_Rank_t other_rank,const gex_Event_t *lc_opt,gex_Flags_t flags,unsigned int numargs){ return gex_AM_MaxReplyLong(tm,other_rank,lc_opt,flags,numargs); }
extern size_t gex_AM_MaxRequestMedium_Wrap(gex_TM_t tm,gex_Rank_t other_rank,const gex_Event_t *lc_opt,gex_Flags_t flags,unsigned int numargs){ return gex_AM_MaxRequestMedium(tm,other_rank,lc_opt,flags,numargs); }
extern size_t gex_AM_MaxReplyMedium_Wrap(gex_TM_t tm,gex_Rank_t other_rank,const gex_Event_t *lc_opt,gex_Flags_t flags,unsigned int numargs){ return gex_AM_MaxReplyMedium(tm,other_rank,lc_opt,flags,numargs); }
extern size_t gex_Token_MaxReplyLong_Wrap(gex_Token_t token,const gex_Event_t *lc_opt,gex_Flags_t flags,unsigned int numargs){ return gex_Token_MaxReplyLong(token,lc_opt,flags,numargs); }
extern size_t gex_Token_MaxReplyMedium_Wrap(gex_Token_t token,const gex_Event_t *lc_opt,gex_Flags_t flags,unsigned int numargs){ return gex_Token_MaxReplyMedium(token,lc_opt,flags,numargs); }
extern size_t gex_AM_LUBRequestLong_Wrap(void){ return gex_AM_LUBRequestLong(); }
extern size_t gex_AM_LUBReplyLong_Wrap(void){ return gex_AM_LUBReplyLong(); }
extern size_t gex_AM_LUBRequestMedium_Wrap(void){ return gex_AM_LUBRequestMedium(); }
extern size_t gex_AM_LUBReplyMedium_Wrap(void){ return gex_AM_LUBReplyMedium(); }
extern gex_TI_t gex_Token_Info_Wrap(gex_Token_t token,gex_Token_Info_t *info,gex_TI_t mask){ return gex_Token_Info(token,info,mask); }
extern void *gex_AM_SrcDescAddr_Wrap(gex_AM_SrcDesc_t sd){ return gex_AM_SrcDescAddr(sd); }
extern size_t gex_AM_SrcDescSize_Wrap(gex_AM_SrcDesc_t sd){ return gex_AM_SrcDescSize(sd); }
extern gex_AM_SrcDesc_t gex_AM_PrepareRequestMedium_Wrap(gex_TM_t tm,gex_Rank_t rank,const void *client_buf,size_t least_payload,size_t most_payload,gex_Event_t *lc_opt,gex_Flags_t flags,unsigned int numargs){ return gex_AM_PrepareRequestMedium(tm,rank,client_buf,least_payload,most_payload,lc_opt,flags,numargs); }
extern gex_AM_SrcDesc_t gex_AM_PrepareReplyMedium_Wrap(gex_Token_t token,const void *client_buf,size_t least_payload,size_t most_payload,gex_Event_t *lc_opt,gex_Flags_t flags,unsigned int numargs){ return gex_AM_PrepareReplyMedium(token,client_buf,least_payload,most_payload,lc_opt,flags,numargs); }
extern gex_AM_SrcDesc_t gex_AM_PrepareRequestLong_Wrap(gex_TM_t tm,gex_Rank_t rank,const void *client_buf,size_t least_payload,size_t most_payload,void *dest_addr,gex_Event_t *lc_opt,gex_Flags_t flags,unsigned int numargs){ return gex_AM_PrepareRequestLong(tm,rank,client_buf,least_payload,most_payload,dest_addr,lc_opt,flags,numargs); }
extern gex_AM_SrcDesc_t gex_AM_PrepareReplyLong_Wrap(gex_Token_t token,const void *client_buf,size_t least_payload,size_t most_payload,void *dest_addr,gex_Event_t *lc_opt,gex_Flags_t flags,unsigned int numargs){ return gex_AM_PrepareReplyLong(token,client_buf,least_payload,most_payload,dest_addr,lc_opt,flags,numargs); }
// const void * cast to void * here. This is a bug in gasnet
extern int gex_RMA_PutBlocking_Wrap(gex_TM_t tm,gex_Rank_t rank,void *dest,const void *src,size_t nbytes,gex_Flags_t flags){ return gex_RMA_PutBlocking(tm,rank,dest,(void *)src,nbytes,flags); }
extern int gex_RMA_PutNBI_Wrap(gex_TM_t tm,gex_Rank_t rank,void *dest,const void *src,size_t nbytes,gex_Event_t *lc_opt,gex_Flags_t flags){ return gex_RMA_PutNBI(tm,rank,dest,(void *)src,nbytes,lc_opt,flags); }
extern gex_Event_t gex_RMA_PutNB_Wrap(gex_TM_t tm,gex_Rank_t rank,void *dest,const void *src,size_t nbytes,gex_Event_t *lc_opt,gex_Flags_t flags){ return gex_RMA_PutNB(tm,rank,dest,(void *)src,nbytes,lc_opt,flags); }
extern int gex_RMA_GetBlocking_Wrap(gex_TM_t tm,void *dest,gex_Rank_t rank,void *src,size_t nbytes,gex_Flags_t flags){ return gex_RMA_GetBlocking(tm,dest,rank,src,nbytes,flags); }
extern int gex_RMA_GetNBI_Wrap(gex_TM_t tm,void *dest,gex_Rank_t rank,void *src,size_t nbytes,gex_Flags_t flags){ return gex_RMA_GetNBI(tm,dest,rank,src,nbytes,flags); }
extern gex_Event_t gex_RMA_GetNB_Wrap(gex_TM_t tm,void *dest,gex_Rank_t rank,void *src,size_t nbytes,gex_Flags_t flags){ return gex_RMA_GetNB(tm,dest,rank,src,nbytes,flags); }
extern gex_RMA_Value_t gex_RMA_GetBlockingVal_Wrap(gex_TM_t tm,gex_Rank_t rank,void *src,size_t nbytes,gex_Flags_t flags){ return gex_RMA_GetBlockingVal(tm,rank,src,nbytes,flags); }
extern int gex_RMA_PutBlockingVal_Wrap(gex_TM_t tm,gex_Rank_t rank,void *dest,gex_RMA_Value_t value,size_t nbytes,gex_Flags_t flags){ return gex_RMA_PutBlockingVal(tm,rank,dest,value,nbytes,flags); }
extern int gex_RMA_PutNBIVal_Wrap(gex_TM_t tm,gex_Rank_t rank,void *dest,gex_RMA_Value_t value,size_t nbytes,gex_Flags_t flags){ return gex_RMA_PutNBIVal(tm,rank,dest,value,nbytes,flags); }
extern gex_Event_t gex_RMA_PutNBVal_Wrap(gex_TM_t tm,gex_Rank_t rank,void *dest,gex_RMA_Value_t value,size_t nbytes,gex_Flags_t flags){ return gex_RMA_PutNBVal(tm,rank,dest,value,nbytes,flags); }
extern void gex_NBI_BeginAccessRegion_Wrap(gex_Flags_t flags){ gex_NBI_BeginAccessRegion(flags); }
extern gex_Event_t gex_NBI_EndAccessRegion_Wrap(gex_Flags_t flags){ return gex_NBI_EndAccessRegion(flags); }
extern int gex_Event_Test_Wrap (gex_Event_t event){ return gex_Event_Test(event); }
extern void gex_Event_Wait_Wrap (gex_Event_t event){ gex_Event_Wait(event); }
extern int gex_Event_TestSome_Wrap (gex_Event_t *pevent, size_t numevents, gex_Flags_t flags){ return gex_Event_TestSome(pevent,numevents,flags); }
extern void gex_Event_WaitSome_Wrap (gex_Event_t *pevent, size_t numevents, gex_Flags_t flags){ gex_Event_WaitSome(pevent,numevents,flags); }
extern int gex_Event_TestAll_Wrap (gex_Event_t *pevent, size_t numevents, gex_Flags_t flags){ return gex_Event_TestAll(pevent,numevents,flags); }
extern void gex_Event_WaitAll_Wrap (gex_Event_t *pevent, size_t numevents, gex_Flags_t flags){ gex_Event_WaitAll(pevent,numevents,flags); }
extern int gex_NBI_Test_Wrap(gex_EC_t event_mask, gex_Flags_t flags){ return gex_NBI_Test(event_mask,flags); }
extern void gex_NBI_Wait_Wrap(gex_EC_t event_mask, gex_Flags_t flags){ gex_NBI_Wait(event_mask,flags); }
extern gex_Event_t gex_Event_QueryLeaf_Wrap(gex_Event_t root,gex_EC_t event_category){ return gex_Event_QueryLeaf(root,event_category); }
extern void gex_System_QueryNbrhdInfo_Wrap(gex_RankInfo_t **info_p,gex_Rank_t *info_count_p,gex_Rank_t *my_info_index_p){ gex_System_QueryNbrhdInfo(info_p,info_count_p,my_info_index_p); }
extern void gex_System_QueryHostInfo_Wrap(gex_RankInfo_t **info_p,gex_Rank_t *info_count_p,gex_Rank_t *my_info_index_p){ gex_System_QueryHostInfo(info_p,info_count_p,my_info_index_p); }
extern void gex_System_QueryMyPosition_Wrap(gex_Rank_t *nbrhd_set_size,gex_Rank_t *nbrhd_set_rank,gex_Rank_t *host_set_size,gex_Rank_t *host_set_rank){ gex_System_QueryMyPosition(nbrhd_set_size,nbrhd_set_rank,host_set_size,host_set_rank); }

extern uintptr_t gasnet_getMaxLocalSegmentSize_Wrap(){ return gasnet_getMaxLocalSegmentSize();}
