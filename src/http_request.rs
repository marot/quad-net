//! Async http requests.

#[cfg(target_arch = "wasm32")]
use sapp_jsutils::JsObject;
use std::future::Future;
use std::task::{Context, Poll, Waker};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum Method {
    Post,
    Put,
    Get,
    Delete,
}

#[derive(Debug)]
pub enum HttpError {
    IOError,
    #[cfg(not(target_arch = "wasm32"))]
    UreqError(ureq::Error),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::IOError => write!(f, "IOError"),
            #[cfg(not(target_arch = "wasm32"))]
            HttpError::UreqError(error) => write!(f, "Ureq error: {}", error),
        }
    }
}
impl From<std::io::Error> for HttpError {
    fn from(_error: std::io::Error) -> HttpError {
        HttpError::IOError
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ureq::Error> for HttpError {
    fn from(error: ureq::Error) -> HttpError {
        HttpError::UreqError(error)
    }
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn http_make_request(scheme: i32, url: JsObject, body: JsObject, headers: JsObject) -> i32;
    fn http_try_recv(cid: i32) -> JsObject;
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Request {
    shared_state: Arc<Mutex<SharedState>>
}

struct SharedState {
    rx: std::sync::mpsc::Receiver<Result<String, HttpError>>,
    waker: Option<Waker>,
}

impl Future for Request {
    type Output = Result<String, HttpError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut shared_state = self.shared_state.lock().unwrap();
        if let Some(result) = shared_state.rx.try_recv().ok() {
            return Poll::Ready(result)
        }

        shared_state.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Request {
    // pub fn try_recv(&mut self) -> Option<Result<String, HttpError>> {
    //     self.rx.try_recv().ok()
    // }
}

#[cfg(target_arch = "wasm32")]
pub struct Request {
    cid: i32,
}

#[cfg(target_arch = "wasm32")]
impl Request {
    pub fn try_recv(&mut self) -> Option<Result<String, HttpError>> {
        let js_obj = unsafe { http_try_recv(self.cid) };

        if js_obj.is_nil() == false {
            let mut buf = vec![];
            js_obj.to_byte_buffer(&mut buf);

            let res = std::str::from_utf8(&buf).unwrap().to_owned();
            return Some(Ok(res));
        }

        None
    }
}

pub struct RequestBuilder {
    url: String,
    method: Method,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

impl RequestBuilder {
    pub fn new(url: &str) -> RequestBuilder {
        RequestBuilder {
            url: url.to_owned(),
            method: Method::Get,
            headers: vec![],
            body: None,
        }
    }

    pub fn method(self, method: Method) -> RequestBuilder {
        Self { method, ..self }
    }

    pub fn header(mut self, header: &str, value: &str) -> RequestBuilder {
        self.headers.push((header.to_owned(), value.to_owned()));

        Self {
            headers: self.headers,
            ..self
        }
    }

    pub fn body(self, body: &str) -> RequestBuilder {
        RequestBuilder {
            body: Some(body.to_owned()),
            ..self
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn send(self) -> Request {
        use std::sync::mpsc::channel;

        let (tx, rx) = channel();
        let request = Request { shared_state: Arc::new(Mutex::new(SharedState { rx, waker: None })) };

        std::thread::spawn({
            let state = request.shared_state.clone();
            move || {
                let method = match self.method {
                    Method::Post => ureq::post,
                    Method::Put => ureq::put,
                    Method::Get => ureq::get,
                    Method::Delete => ureq::delete,
                };

                let mut request = method(&self.url);
                for (header, value) in self.headers {
                    request = request.set(&header, &value)
                }
                let response: Result<String, HttpError> = if let Some(body) = self.body {
                    request.send_string(&body)
                } else {
                    request.call()
                }
                    .map_err(|err| err.into())
                    .and_then(|response| response.into_string().map_err(|err| err.into()));

                tx.send(response).unwrap();
                let mut shared_state = state.lock().unwrap();
                if let Some(waker) = shared_state.waker.take() {
                    waker.wake();
                }
            }
        });

        request
    }

    #[cfg(target_arch = "wasm32")]
    pub fn send(&self) -> Request {
        let scheme = match self.method {
            Method::Post => 0,
            Method::Put => 1,
            Method::Get => 2,
            Method::Delete => 3,
        };

        let headers = JsObject::object();

        for (header, value) in &self.headers {
            headers.set_field_string(&header, &value);
        }

        let cid = unsafe {
            http_make_request(
                scheme,
                JsObject::string(&self.url),
                JsObject::string(&self.body.as_ref().map(|s| s.as_str()).unwrap_or("")),
                headers,
            )
        };
        Request { cid }
    }
}
