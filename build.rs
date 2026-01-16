fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 编译 proto 文件，为生成的类型添加 Serde derive
    // 这样 protobuf 类型可以同时使用 Serde 和 prost::Message
    prost_build::Config::new()
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile_protos(&["proto/test_message.proto"], &["proto/"])?;

    // 告诉 cargo 当 proto 文件发生变化时重新构建
    println!("cargo:rerun-if-changed=proto/");

    Ok(())
}