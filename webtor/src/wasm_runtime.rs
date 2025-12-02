use crate::time::system_time_now;
use std::time::Duration;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tor_rtcompat::{CoarseTimeProvider, SleepProvider};
use tor_rtcompat::CoarseInstant;
use tor_rtcompat::RealCoarseTimeProvider;

#[derive(Clone, Debug)]
pub struct WasmRuntime {
    coarse: RealCoarseTimeProvider,
}

impl WasmRuntime {
    pub fn new() -> Self {
        Self {
            coarse: RealCoarseTimeProvider::new(),
        }
    }
}

impl CoarseTimeProvider for WasmRuntime {
    fn now_coarse(&self) -> CoarseInstant {
        self.coarse.now_coarse()
    }
}

impl SleepProvider for WasmRuntime {
    type SleepFuture = WasmSleep;

    fn sleep(&self, duration: Duration) -> Self::SleepFuture {
        WasmSleep::new(duration)
    }

    fn now(&self) -> tor_rtcompat::PortableInstant {
        tor_rtcompat::PortableInstant::now()
    }

    fn wallclock(&self) -> std::time::SystemTime {
        system_time_now()
    }
}

pub struct WasmSleep {
    rx: futures::channel::oneshot::Receiver<()>,
}

impl WasmSleep {
    fn new(duration: Duration) -> Self {
        let (tx, rx) = futures::channel::oneshot::channel();
        
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::prelude::*;
            use wasm_bindgen::JsCast;
            
            let millis = duration.as_millis() as i32;
            
            let closure = Closure::once(move || {
                let _ = tx.send(());
            });
            
            let window = web_sys::window().expect("should have a window in this context");
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                closure.as_ref().unchecked_ref(),
                millis,
            );
            
            closure.forget(); 
        }
        
        #[cfg(not(target_arch = "wasm32"))]
        {
            let duration = duration.clone();
            std::thread::spawn(move || {
                std::thread::sleep(duration);
                let _ = tx.send(());
            });
        }

        Self { rx }
    }
}

impl Future for WasmSleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use futures::FutureExt;
        match self.rx.poll_unpin(cx) {
            Poll::Ready(_) => Poll::Ready(()),
            Poll::Pending => Poll::Pending,
        }
    }
}
