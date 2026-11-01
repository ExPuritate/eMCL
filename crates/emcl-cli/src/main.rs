use emcl_core::environment::home_dir;

fn main() {
    println!("{}", home_dir().display());
}
