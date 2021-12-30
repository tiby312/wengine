use gloo::console::log;
use gloo::timers::future::TimeoutFuture;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

mod circle_program;
pub mod dots;

pub mod utils {
    //!
    //! Helper functions to access elements
    //!
    use super::*;
    pub fn get_by_id_canvas(id: &str) -> web_sys::HtmlCanvasElement {
        gloo::utils::document()
            .get_element_by_id(id)
            .unwrap_throw()
            .dyn_into()
            .unwrap_throw()
    }
    pub fn get_by_id_elem(id: &str) -> web_sys::HtmlElement {
        gloo::utils::document()
            .get_element_by_id(id)
            .unwrap_throw()
            .dyn_into()
            .unwrap_throw()
    }

    pub fn get_context_webgl2_offscreen(
        canvas: &web_sys::OffscreenCanvas,
    ) -> web_sys::WebGl2RenderingContext {
        canvas
            .get_context("webgl2")
            .unwrap_throw()
            .unwrap_throw()
            .dyn_into()
            .unwrap_throw()
    }
}

#[wasm_bindgen]
extern "C" {
    #[no_mangle]
    #[used]
    static performance: web_sys::Performance;
}

struct Timer {
    last: f64,
    frame_rate: usize,
}
impl Timer {
    fn new(frame_rate: usize) -> Timer {
        let frame_rate = ((1.0 / frame_rate as f64) * 1000.0).round() as usize;

        assert!(frame_rate > 0);
        //let window = gloo::utils::window();
        //let performance = window.performance().unwrap_throw();

        Timer {
            last: performance.now(),
            frame_rate,
        }
    }

    async fn next(&mut self) {
        //let window = gloo::utils::window();
        //let performance = window.performance().unwrap_throw();

        let tt = performance.now();
        let diff = performance.now() - self.last;

        if self.frame_rate as f64 - diff > 0.0 {
            let d = (self.frame_rate as f64 - diff) as usize;
            TimeoutFuture::new(d.try_into().unwrap_throw()).await;
        }

        self.last = tt;
    }
}


use futures::Stream;
use futures::StreamExt;
use futures::FutureExt;


pub struct FrameTimer<T,K>{
    timer:Timer,
    buffer:Vec<T>,
    stream:K
}
impl<T,K:Stream<Item=T>+std::marker::Unpin> FrameTimer<T,K>{
    pub fn new(frame_rate:usize,stream:K)->Self{
        FrameTimer{
            timer:Timer::new(frame_rate),
            buffer:vec![],
            stream
        }
    }
    pub async fn next(&mut self)->&[T]{
        loop{
            futures::select_biased!(
                _ = self.timer.next().fuse() =>{
                    break;
                },
                val = self.stream.next().fuse()=>{
                    self.buffer.push(val.unwrap_throw());
                }
            )

        }
        &self.buffer
    }
}





pub use main::EngineMain;
use std::marker::PhantomData;
mod main {
    use super::*;
    ///
    /// The component of the engine that runs on the main thread.
    ///
    pub struct EngineMain<T,K> {
        worker: std::rc::Rc<std::cell::RefCell<web_sys::Worker>>,
        shutdown_fr: futures::channel::oneshot::Receiver<()>,
        _handle: gloo::events::EventListener,
        _p: PhantomData<(T,K)>,
    }

    impl<T: 'static + Serialize,K:for<'a>Deserialize<'a>+'static> EngineMain<T,K> {
        ///
        /// Create the engine. Blocks until the worker thread reports that
        /// it is ready to receive the offscreen canvas.
        ///
        pub async fn new(canvas: web_sys::OffscreenCanvas) -> (Self,futures::channel::mpsc::UnboundedReceiver<K>) {
            let mut options = web_sys::WorkerOptions::new();
            options.type_(web_sys::WorkerType::Module);
            let worker = Rc::new(RefCell::new(
                web_sys::Worker::new_with_options("./worker.js", &options).unwrap(),
            ));

            let (shutdown_fs, shutdown_fr) = futures::channel::oneshot::channel();
            let mut shutdown_fs = Some(shutdown_fs);

            let (fs, fr) = futures::channel::oneshot::channel();
            let mut fs = Some(fs);

            let (ks,kr)=futures::channel::mpsc::unbounded();
            let _handle =
                gloo::events::EventListener::new(&worker.borrow(), "message", move |event| {
                    let event = event.dyn_ref::<web_sys::MessageEvent>().unwrap_throw();
                    let data = event.data();

                    let data:js_sys::Array = data.dyn_into().unwrap_throw();
                    let m=data.get(0);
                    let k=data.get(1);

                    if !m.is_null(){
                        if let Some(s) = m.as_string() {
                        
                            if s == "ready" {
                        
                                if let Some(f) = fs.take() {
                                    f.send(()).unwrap_throw();
                                }
                            } else if s == "close" {
                                if let Some(f) = shutdown_fs.take() {
                                    f.send(()).unwrap_throw();
                                }
                            }
                        }
                    }

                    if !k.is_null(){
                        ks.unbounded_send(k.into_serde().unwrap_throw()).unwrap_throw();
                    }

                });

            let _ = fr.await.unwrap_throw();
            log!("main:got ready response!");

            let arr = js_sys::Array::new_with_length(1);
            arr.set(0, canvas.clone().into());

            let data=js_sys::Array::new();
            data.set(0,canvas.into());
            data.set(1,JsValue::null());

            worker
                .borrow()
                .post_message_with_transfer(&data, &arr)
                .unwrap_throw();

            (EngineMain {
                worker,
                shutdown_fr,
                _handle,
                _p: PhantomData,
            },kr)
        }

        ///
        /// Block until the worker thread returns.
        ///
        pub async fn join(self) {
            let _ = self.shutdown_fr.await.unwrap_throw();
        }

        pub fn send_event(&mut self,val:T){
            let a = JsValue::from_serde(&val).unwrap_throw();

            let data=js_sys::Array::new();
            data.set(0,JsValue::null());
            data.set(1,a);

            self.worker.borrow().post_message(&data).unwrap_throw();
        }

        ///
        /// Register a new event that will be packaged and sent to the worker thread.
        ///
        pub fn register_event(
            &mut self,
            elem: &web_sys::HtmlElement,
            event_type: &'static str,
            mut func: impl FnMut(EventData) -> T + 'static,
        ) -> gloo::events::EventListener {
            let w = self.worker.clone();

            let e = elem.clone();
            gloo::events::EventListener::new(&elem, event_type, move |event| {
                let e = EventData {
                    elem: &e,
                    event,
                    event_type,
                };

                let val = func(e);
                let a = JsValue::from_serde(&val).unwrap_throw();


                let data=js_sys::Array::new();
                data.set(0,JsValue::null());
                data.set(1,a);

                
                w.borrow().post_message(&data).unwrap_throw();
            })
        }
    }
}

///
/// Data that can be accessed when handling events in the main thread to help
/// construct the data to be passed to the worker thread.
///
pub struct EventData<'a> {
    pub elem: &'a web_sys::HtmlElement,
    pub event: &'a web_sys::Event,
    pub event_type: &'static str,
}

pub use worker::EngineWorker;
mod worker {
    use super::*;
    ///
    /// The component of the engine that runs on the worker thread spawn inside of worker.js.
    ///
    pub struct EngineWorker<T,K> {
        _handle: gloo::events::EventListener,
        queue: Rc<RefCell<Vec<T>>>,
        buffer: Vec<T>,
        timer: crate::Timer,
        canvas: Rc<RefCell<Option<web_sys::OffscreenCanvas>>>,
        _p:PhantomData<K>
    }

    impl<T,K> Drop for EngineWorker<T,K> {
        fn drop(&mut self) {
            let scope: web_sys::DedicatedWorkerGlobalScope =
                js_sys::global().dyn_into().unwrap_throw();


            let data = js_sys::Array::new();
            data.set(0,JsValue::from_str("close"));
            data.set(1,JsValue::null());

            scope
                .post_message(&data)
                .unwrap_throw();
        }
    }
    impl<T: 'static + for<'a> Deserialize<'a>,K:Serialize> EngineWorker<T,K> {
        ///
        /// Get the offscreen canvas.
        ///
        pub fn canvas(&self) -> web_sys::OffscreenCanvas {
            self.canvas.borrow().as_ref().unwrap_throw().clone()
        }

        ///
        /// Create the worker component of the engine.
        /// Specify the frame rate.
        /// Blocks until it receives the offscreen canvas from the main thread.
        ///
        pub async fn new(time: usize) -> EngineWorker<T,K> {
            let scope: web_sys::DedicatedWorkerGlobalScope =
                js_sys::global().dyn_into().unwrap_throw();

            let queue: Rc<RefCell<Vec<T>>> = std::rc::Rc::new(std::cell::RefCell::new(vec![]));

            let ca: Rc<RefCell<Option<web_sys::OffscreenCanvas>>> =
                std::rc::Rc::new(std::cell::RefCell::new(None));

            let (fs, fr) = futures::channel::oneshot::channel();
            let mut fs = Some(fs);

            let caa = ca.clone();
            let q = queue.clone();

            let _handle = gloo::events::EventListener::new(&scope, "message", move |event| {
                let event = event.dyn_ref::<web_sys::MessageEvent>().unwrap_throw();
                let data = event.data();

                let data:js_sys::Array=data.dyn_into().unwrap_throw();
                let offscreen=data.get(0);
                let payload=data.get(1);

                if !offscreen.is_null(){
                    
                    let offscreen:web_sys::OffscreenCanvas = offscreen.dyn_into().unwrap_throw();
                    *caa.borrow_mut() = Some(offscreen);
                    if let Some(fs) = fs.take() {
                        fs.send(()).unwrap_throw();
                    }
                }

                if !payload.is_null() {
                    let e = payload.into_serde().unwrap_throw();

                    q.borrow_mut().push(e);
                    
                }
            });


            let data = js_sys::Array::new();
            data.set(0,JsValue::from_str("ready"));
            data.set(1,JsValue::null());

            scope
                .post_message(&data)
                .unwrap_throw();
            log!("worker:sent ready");

            fr.await.unwrap_throw();

            log!("worker:ready to continue");
            EngineWorker {
                _handle,
                queue,
                buffer: vec![],
                timer: crate::Timer::new(time),
                canvas: ca,
                _p:PhantomData
            }
        }

        pub fn post_message(&mut self,a:K){
            let scope: web_sys::DedicatedWorkerGlobalScope =
                js_sys::global().dyn_into().unwrap_throw();

            let data = js_sys::Array::new();
            data.set(0,JsValue::null());
            data.set(1,JsValue::from_serde(&a).unwrap_throw());

            scope.post_message(&data).unwrap_throw();
        }

        ///
        /// Blocks until the next frame. Returns all events that
        /// transpired since the previous call to next.
        ///
        pub async fn next(&mut self) -> &[T] {
            self.timer.next().await;
            self.buffer.clear();
            self.buffer.append(&mut self.queue.borrow_mut());
            &self.buffer
        }
    }
}
