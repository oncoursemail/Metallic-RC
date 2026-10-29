fn main(){
 if std::env::var("CARGO_CFG_TARGET_OS").as_deref()==Ok("windows"){
  assert_eq!(std::env::var("CARGO_CFG_TARGET_ENV").as_deref(),Ok("msvc"),"reviewed MSVC guard build required");
  println!("cargo:rustc-link-arg-bin=metallic-medium-guard=/DEPENDENTLOADFLAG:0x800");
 }
}
