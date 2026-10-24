use wasm_bindgen::prelude::*;
use web_sys::window;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    log("Starting LEDFX Editor");
    
    let web_options = eframe::WebOptions::default();
    wasm_bindgen_futures::spawn_local(async {
        let document = window()
            .expect("No window")
            .document()
            .expect("No document");
        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id was not a HtmlCanvasElement");
        
        eframe::WebRunner::new()
            .start(
                "the_canvas_id",
                web_options,
                Box::new(|_cc| Ok(Box::new(crate::LedFxApp::default()))),
            )
            .await
            .expect("Failed to start eframe");
    });
}

pub fn download_file(filename: &str, data: &[u8], mime_type: &str) {
    let window = match window() {
        Some(w) => w,
        None => return,
    };
    let document = window.document().unwrap();
    let js_array = js_sys::Array::new();
    let uint8_array = js_sys::Uint8Array::new_with_length(data.len() as u32);
    for (i, &b) in data.iter().enumerate() {
        uint8_array.set_index(i as u32, b);
    }
    js_array.push(&uint8_array);
    let mut blob_options = web_sys::BlobPropertyBag::new();
    blob_options.set_type(mime_type);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&js_array, &blob_options).unwrap();
    let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap();
    let a = document.create_element("a").unwrap();
    let a: web_sys::HtmlAnchorElement = a.dyn_into().unwrap();
    a.set_href(&url);
    a.set_download(filename);
    let html_element: &web_sys::HtmlElement = a.as_ref();
    html_element.click();
    let _ = web_sys::Url::revoke_object_url(&url);
}
