
#![allow(dead_code)]
mod dead{
async fn real_fn(&mut ctx: Context, a: i32, b: A, c: i32) -> usize { // macro
    0
}

// block until real function finished
async fn execute_and_send_fn0(my_activity_id:ActivityId, waited: bool, a: i32, b: A, c: i32) { // macro
    let finish_id = my_activity_id.get_finish_id();
    let mut ctx = ConcreteContext::inherit(finish_id);
    let future = real_fn(&mut ctx, a, b, c); //macro
    let result = future.catch_unwind().await;

    // TODO panic?
    // should set dst place of return to it's finishid, to construct calling tree
    let mut builder = SquashBufferItemBuilder::new(fn_id, finish_id.get_place(), my_activity_id);
    spwaned_activities = ctx.spawned(); // get activitiy spwaned in real_fn
    builder.ret(result); // macro TODO: strip return value
    builder.sub_activities(spwaned_activities);
    let item = builder.build();
    Context::send(item); 
    // send to the place waited (spwaned)
    if waited{
        // two ret must be identical if dst is the same place
        let mut builder = SquashBufferItemBuilder::new(fn_id, my_activity_id.get_spawned_place(), my_activity_id);
        builder.ret(result); // macro
        builder.sub_activities(spwaned_activities);
        let item = builder.build();
        Context::send(item); 
    }
}


// the one executed by worker
async fn real_fn_wrap_execute_from_remote(item: SquashBufferItem) {
    let waited = item.is_waited();
    let mut e = SquashBufferItemExtracter::new(item);
    let my_activity_id = e.activity();
    let fn_id = e.fn_id();
    let finish_id = get_finish_id(my_activity_id);

    // wait until function return
    execute_and_send_fn0(my_activity_id, waited, e.arg(), e.arg_squash(), e.arg()).await; // macro 
}

// the desugered at async and wait
fn async_create_for_fn_id_0(ctx:&mut Context, dst_place: Place, a: i32, b: A, c: i32) -> impl future::Future<Output=A>{ // macro
    let my_activity_id = ctx.spawn(); // register to remote
    let fn_id: FunctionLabel = 0; // macro

    let f = wait_single::<A>(my_activity_id); // macro
    if dst_place == here {
       tokio::spwan(execute_and_send_fn0(my_activity_id, true, a, b, c)); // macro
    } else {
        let mut builder = SquashBufferItemBuilder::new(fn_id, dst_place, my_activity_id);
        builder.arg(a); //  macro
        builder.arg_squash(b); // macro
        builder.arg(c); //macro
        builder.waited();
        let item = builder.build();
        Context::send(item);
    }
    f
}

// the desugered at async no wait
fn async_create_no_wait_for_fn_id_0(&mut ctx:Context, dst_place: Place, a: i32, b: A, c: i32) { // macro
    let my_activity_id = ctx.spawn(); // register to remote
    let fn_id: FunctionLabel = 0; // macro

    if dst_place == here {
        // no wait, set flag = flase
       tokio::spwan(execute_and_send_fn0(my_activity_id, false, a, b, c)); // macro
    } else {
        let mut builder = SquashBufferItemBuilder::new(fn_id, dst_place, my_activity_id);
        builder.arg(a); //  macro
        builder.arg_squash(b); // macro
        builder.arg(c); //macro
        let item = builder.build();
        Context::send(item);
    }
}

// desugered finish
async fn finish() {
    let ctx = ctx.new_frame();
    // ctx contains a new finish id now
    let f = async_crate_for_fn_id_0(&mut ctx, dst_place, 1, A { value: 3 }, 3);

    let ret = f.await; // if await, remove it from activity list this finish block will wait
    println!("{}", ret);
    async_crate_for_fn_id_0(&mut ctx, dst_place, 1, A { value: 3 }, 3);

    ctx.wait_all().await;
}

pub fn main() {
    


    // local async
    let a = here_async!{
    }; // => desugared as
    let a = async {};



    // at place block
    let c = at!(place, {
        // work
    });
    // should be desugar as
    let c = at_async!(place, {
        // work
    }).await;

    // at place async, b is a future
    let b = at_async!(place, 
        {
            // #expr
        }
    ); // will be desugared as
    let b = if (place == local){
        async {
            expr
        }
    } else{
        let top_frame = this_activity.get_top_frame();
        let args = arg_list; // pack & serialize all args. Can not pack as ref here, since lifetime is in the scope

        #[derive(Serialize, Deserialize)]
        struct Args_anonymous(a1, a2, a3);//...

        // TODO generating global id for each activity id = place + worker_no + time? 
        // 
        /* going to generate a global function, with an id
        fn (this_activity, args){
            this_activity.get_top_frame();
            expr
        }
        */

        // there should be a table to record the outgoint calls for each thread
        let handler = run_at(place, top_frame, function_id, args);
        handler.retrive() // return a future
    };


    finish!{

        // code1

        here_async!{ };

        // code2

        at_async!(place, { });

        finish!{};

        let c = at!(place,{});

        // code4
        
        let d = at_async!(place, {});

        let e = d.await;

    }; // => should be desugar as
    {
        global_finish_stack.push();

        // all async not bind/await to a name are moved to first. that ensures will be executed before any blocking callesk
        // Async activities bind to a name is assumed to live no longer than the scope
        // also ensure async only capture vars outside the finish
        let _i = here_async!{ }; 

        let _j = at_async!(place, async{ });

        // code1
        // code2
        finish!{};
        let c = at!(place,{}); // blocked, no change
        // code4
        
        let d = at_async!(place, {});

        let e = d.await;

        _i.await; // wait all local

        this_frame.wait_all().await; // wait all remote, this is notify by task_manager.

        global_finish_stack.pop()
    
    };

    finish!{

        here_async!{ // asyn1
            hera_async!{ // asyn2

            };
            at_async!(place, { // at_async1
                at_async!(place, {}); // at_async2
            });
        }

    } // => should be desugar as
    {
        global_finish_stack.push();
        let _i = here_async!{}; //asyn1;
        let _j = here_async!{}; //asyn2;
        let _k = at_async!{ //at_async1;
            at_async!{  // at_async2;
            }
        };
        global_finish_stack.pop();
    }
    // rule: define here_async/at_async without await on them as orphan acctivity
    // 1. all direct orphan activities in a finish block will be moved to the beginning, and then
    //    awaited at the end of the finish block.
    // 2. all orphan activities in a orphan here_async activities will be moved to the beginning of
    //    the nearest finish block, and then awaited at the end of the finish block.
    
}
}
