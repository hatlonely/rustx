fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 编译 test_message.proto，使用 prost-build
    // 为生成的类型添加 Serde derive
    prost_build::Config::new()
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile_protos(&["proto/test_message.proto"], &["proto/"])?;

    // 编译 echo.proto，使用 tonic-prost-build 生成 gRPC 服务代码
    tonic_prost_build::configure()
        .compile_protos(&["proto/echo.proto"], &["proto/"])?;

    // 告诉 cargo 当 proto 文件发生变化时重新构建
    println!("cargo:rerun-if-changed=proto/");

    // 获取 git tag 并嵌入到编译产物中
    if let Ok(output) = std::process::Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
    {
        if output.status.success() {
            let git_version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("cargo:rustc-env=GIT_VERSION={}", git_version);
        }
    }

    Ok(())
}