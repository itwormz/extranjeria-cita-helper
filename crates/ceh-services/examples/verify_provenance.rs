//! CI uses exactly the application's offline package provenance policy.
fn main(){
 let args:Vec<_>=std::env::args().skip(1).collect();
 if args.len()!=4{eprintln!("usage: verify_provenance <package> <version> <repository URL> <source commit>");std::process::exit(2);}
 if ceh_services::attestations::verify_file(std::path::Path::new(&args[0]),&args[1],&args[2],&args[3]).is_err(){eprintln!("Package provenance verification failed");std::process::exit(1);}
}
