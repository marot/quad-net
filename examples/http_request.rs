use quad_net::http_request::RequestBuilder;
use futures::executor::block_on;

fn main() {
    let future = async {
        // let mut request = RequestBuilder::new("http://127.0.0.1:4000/").send();
        let mut request = RequestBuilder::new("http://google.com/").send();

        let result = request.await;

        println!("Done! {:?}", result);
    };

    block_on(future);
}
