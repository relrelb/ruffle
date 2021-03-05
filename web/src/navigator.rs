//! Navigator backend for web
use js_sys::{Array, ArrayBuffer, Uint8Array};
use ruffle_core::backend::navigator::{
    NavigationMethod, NavigatorBackend, OwnedFuture, RequestOptions,
};
use ruffle_core::indexmap::IndexMap;
use ruffle_core::loader::Error;
use std::borrow::Cow;
use std::time::Duration;
use url::Url;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{window, Blob, BlobPropertyBag, Performance, Request, RequestInit, Response};

pub struct WebNavigatorBackend {
    base_url: Url,
    performance: Performance,
    start_time: f64,
    allow_script_access: bool,
    upgrade_to_https: bool,
}

impl WebNavigatorBackend {
    pub fn new(allow_script_access: bool, upgrade_to_https: bool) -> Self {
        let window = web_sys::window().expect("window()");

        let href = window.location().href().expect("href()");
        let base_url = Url::parse(href).unwrap();

        let performance = window.performance().expect("window.performance()");

        // Upgarde to HTTPS takes effect if the current page is hosted on HTTPS.
        let upgrade_to_https =
            upgrade_to_https && window.location().protocol().unwrap_or_default() == "https:";

        WebNavigatorBackend {
            base_url,
            performance,
            start_time: performance.now(),
            allow_script_access,
            upgrade_to_https,
        }
    }

    fn run_script(&self, js_code: &str) {
        if self.allow_script_access {
            let window = web_sys::window().expect("window()");
            let document = window.document().expect("document()");
            let body = document.body().expect("body()");

            let script = document.create_element("script").unwrap();
            script.set_inner_html(&js_code);

            let _ = body.append_child(&script);
            let _ = body.remove_child(&script);
        } else {
            log::error!("SWF tried to run a script, but script access is not allowed");
        }
    }
}

impl NavigatorBackend for WebNavigatorBackend {
    fn navigate_to_url(
        &self,
        url: String,
        window_spec: Option<String>,
        vars_method: Option<(NavigationMethod, IndexMap<String, String>)>,
    ) {
        const JAVASCRIPT_PREFIX: &str = "javascript:";

        if let Some(window) = window() {
            let url_trimmed = url.trim();
            if url_trimmed
                .get(..JAVASCRIPT_PREFIX.len())
                .unwrap_or_default()
                .eq_ignore_ascii_case(JAVASCRIPT_PREFIX)
            {
                let target = window_spec.unwrap_or_else(|| "".to_string());
                if target.is_empty() || target == "_self" || target == "undefined" {
                    self.run_script(&url);
                }
                return;
            }

            let window_url = if url.is_empty() {
                "".to_string()
            } else if url_trimmed.is_empty() {
                "./".to_string()
            } else {
                url.to_string()
            };

            let form_url = if let Ok(parsed_url) = Url::parse(&url) {
                self.pre_process_url(parsed_url).to_string()
            } else {
                url.to_string()
            };

            //TODO: Should we return a result for failed opens? Does Flash care?
            match (vars_method, window_spec) {
                (Some((navmethod, formvars)), window_spec) => {
                    let document = match window.document() {
                        Some(document) => document,
                        None => return,
                    };
                    let body = match document.body() {
                        Some(body) => body,
                        None => return,
                    };

                    let form = document
                        .create_element("form")
                        .unwrap()
                        .dyn_into::<web_sys::HtmlFormElement>()
                        .unwrap();

                    let _ = form.set_attribute(
                        "method",
                        match navmethod {
                            NavigationMethod::Get => "get",
                            NavigationMethod::Post => "post",
                        },
                    );

                    let _ = form.set_attribute("action", &form_url);

                    if let Some(target) = window_spec {
                        let _ = form.set_attribute("target", &target);
                    }

                    for (k, v) in formvars.iter() {
                        let hidden = document.create_element("input").unwrap();

                        let _ = hidden.set_attribute("type", "hidden");
                        let _ = hidden.set_attribute("name", k);
                        let _ = hidden.set_attribute("value", v);

                        let _ = form.append_child(&hidden);
                    }

                    let _ = body.append_child(&form);
                    let _ = form.submit();
                }
                (_, Some(ref window_name)) if !window_name.is_empty() => {
                    if !window_url.is_empty() {
                        let _ = window.open_with_url_and_target(&window_url, window_name);
                    }
                }
                _ => {
                    if !window_url.is_empty() {
                        let _ = window.location().assign(&window_url);
                    }
                }
            };
        }
    }

    fn time_since_launch(&mut self) -> Duration {
        let dt = self.performance.now() - self.start_time;
        Duration::from_millis(dt as u64)
    }

    fn fetch(&self, url: &str, options: RequestOptions) -> OwnedFuture<Vec<u8>, Error> {
        let url = if let Ok(parsed_url) = Url::parse(url) {
            self.pre_process_url(parsed_url).to_string()
        } else {
            url.to_string()
        };

        Box::pin(async move {
            let mut init = RequestInit::new();

            init.method(match options.method() {
                NavigationMethod::Get => "GET",
                NavigationMethod::Post => "POST",
            });

            if let Some((data, mime)) = options.body() {
                let arraydata = ArrayBuffer::new(data.len() as u32);
                let u8data = Uint8Array::new(&arraydata);

                for (i, byte) in data.iter().enumerate() {
                    u8data.fill(*byte, i as u32, i as u32 + 1);
                }

                let blobparts = Array::new();
                blobparts.push(&arraydata);

                let mut blobprops = BlobPropertyBag::new();
                blobprops.type_(mime);

                let datablob =
                    Blob::new_with_buffer_source_sequence_and_options(&blobparts, &blobprops)
                        .unwrap()
                        .dyn_into()
                        .unwrap();

                init.body(Some(&datablob));
            }

            let request = Request::new_with_str_and_init(&url, &init)
                .map_err(|_| Error::FetchError(format!("Unable to create request for {}", url)))?;

            let window = web_sys::window().unwrap();
            let fetchval = JsFuture::from(window.fetch_with_request(&request)).await;
            if fetchval.is_err() {
                return Err(Error::NetworkError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Could not fetch, got JS Error",
                )));
            }

            let resp: Response = fetchval.unwrap().dyn_into().unwrap();
            let data: ArrayBuffer = JsFuture::from(resp.array_buffer().unwrap())
                .await
                .unwrap()
                .dyn_into()
                .unwrap();
            let jsarray = Uint8Array::new(&data);
            let mut rust_array = vec![0; jsarray.length() as usize];
            jsarray.copy_to(&mut rust_array);

            Ok(rust_array)
        })
    }

    fn spawn_future(&mut self, future: OwnedFuture<(), Error>) {
        spawn_local(async move {
            if let Err(e) = future.await {
                log::error!("Asynchronous error occurred: {}", e);
            }
        })
    }

    fn resolve_relative_url<'a>(&mut self, url: &'a str) -> Cow<'a, str> {
        if let Ok(relative) = self.base_url.join(url) {
            relative.into_string().into()
        } else {
            url.into()
        }
    }

    fn pre_process_url(&self, mut url: Url) -> Url {
        if self.upgrade_to_https && url.scheme() == "http" && url.set_scheme("https").is_err() {
            log::error!("Url::set_scheme failed on: {}", url);
        }
        url
    }
}
