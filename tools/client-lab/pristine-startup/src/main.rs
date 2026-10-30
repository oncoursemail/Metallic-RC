fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match metallic_pristine_startup::closed_entrypoint(&args) {
        Ok(plan) => println!("{plan}"),
        Err(error) => { eprintln!("{{\"ok\":false,\"error\":\"{error}\",\"gameWillNotRun\":true,\"guardWillNotRun\":true,\"executionReady\":false}}"); std::process::exit(1); }
    }
}
