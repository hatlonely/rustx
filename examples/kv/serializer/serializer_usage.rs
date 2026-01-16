use rustx::cfg::{create_trait_from_type_options, TypeOptions};
use rustx::kv::serializer::{register_serde_serializers, Serializer};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct User {
    id: u64,
    name: String,
    email: String,
    age: u32,
    active: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let user = User {
        id: 12345,
        name: "张三".to_string(),
        email: "zhangsan@example.com".to_string(),
        age: 28,
        active: true,
    };

    println!("=== Rust kv/serializer 使用示例 ===");
    println!("原始用户数据: {:?}\n", user);

    // 首先注册 User 类型的序列化器
    println!("注册 User 类型的序列化器...");
    register_serde_serializers::<User>()?;
    println!("注册完成 ✓\n");

    // 1. 使用 cfg 创建 JSON 序列化器（美化格式）
    println!("1. JSON 序列化器（美化格式）:");
    let json_opts = TypeOptions::from_json(
        r#"{
        "type": "JsonSerializer",
        "options": { "pretty": true }
    }"#,
    )?;

    let json_serializer: Box<dyn Serializer<User, Vec<u8>>> =
        create_trait_from_type_options(&json_opts)?;

    let json_bytes = json_serializer.serialize(user.clone()).await?;
    println!("JSON 序列化结果:");
    println!("{}", String::from_utf8_lossy(&json_bytes));

    let json_deserialized: User = json_serializer.deserialize(json_bytes.clone()).await?;
    assert_eq!(user, json_deserialized);
    println!("JSON 反序列化成功 ✓\n");

    // 2. 使用 cfg 创建 MessagePack 序列化器
    println!("2. MessagePack 序列化器:");
    let msgpack_opts = TypeOptions::from_json(
        r#"{
        "type": "MsgPackSerializer",
        "options": { "named": true }
    }"#,
    )?;

    let msgpack_serializer: Box<dyn Serializer<User, Vec<u8>>> =
        create_trait_from_type_options(&msgpack_opts)?;

    let msgpack_bytes = msgpack_serializer.serialize(user.clone()).await?;
    println!("MessagePack 序列化字节数: {} bytes", msgpack_bytes.len());

    let msgpack_deserialized: User = msgpack_serializer
        .deserialize(msgpack_bytes.clone())
        .await?;
    assert_eq!(user, msgpack_deserialized);
    println!("MessagePack 反序列化成功 ✓\n");

    // 3. 使用 cfg 创建 BSON 序列化器
    println!("3. BSON 序列化器:");
    let bson_opts = TypeOptions::from_json(
        r#"{
        "type": "BsonSerializer",
        "options": { "utc": true }
    }"#,
    )?;

    let bson_serializer: Box<dyn Serializer<User, Vec<u8>>> =
        create_trait_from_type_options(&bson_opts)?;

    let bson_bytes = bson_serializer.serialize(user.clone()).await?;
    println!("BSON 序列化字节数: {} bytes", bson_bytes.len());

    let bson_deserialized: User = bson_serializer.deserialize(bson_bytes.clone()).await?;
    assert_eq!(user, bson_deserialized);
    println!("BSON 反序列化成功 ✓\n");

    // 4. 使用紧凑格式的 JSON 序列化器
    println!("4. JSON 序列化器（紧凑格式）:");
    let json_compact_opts = TypeOptions::from_json(
        r#"{
        "type": "JsonSerializer",
        "options": { "pretty": false }
    }"#,
    )?;

    let json_compact_serializer: Box<dyn Serializer<User, Vec<u8>>> =
        create_trait_from_type_options(&json_compact_opts)?;

    let json_compact_bytes = json_compact_serializer.serialize(user.clone()).await?;
    println!("紧凑 JSON 序列化结果:");
    println!("{}", String::from_utf8_lossy(&json_compact_bytes));

    let json_compact_deserialized: User = json_compact_serializer
        .deserialize(json_compact_bytes.clone())
        .await?;
    assert_eq!(user, json_compact_deserialized);
    println!("紧凑 JSON 反序列化成功 ✓\n");

    // 5. 无命名字段的 MessagePack 序列化器
    println!("5. MessagePack 序列化器（无命名字段）:");
    let msgpack_no_names_opts = TypeOptions::from_json(
        r#"{
        "type": "MsgPackSerializer",
        "options": { "named": false }
    }"#,
    )?;

    let msgpack_no_names_serializer: Box<dyn Serializer<User, Vec<u8>>> =
        create_trait_from_type_options(&msgpack_no_names_opts)?;

    let msgpack_no_names_bytes = msgpack_no_names_serializer.serialize(user.clone()).await?;
    println!(
        "MessagePack（无命名） 序列化字节数: {} bytes",
        msgpack_no_names_bytes.len()
    );

    let msgpack_no_names_deserialized: User = msgpack_no_names_serializer
        .deserialize(msgpack_no_names_bytes.clone())
        .await?;
    assert_eq!(user, msgpack_no_names_deserialized);
    println!("MessagePack（无命名） 反序列化成功 ✓\n");

    // 6. 性能对比
    println!("6. 序列化字节数对比:");
    println!("JSON (美化):      {} bytes", json_bytes.len());
    println!("JSON (紧凑):      {} bytes", json_compact_bytes.len());
    println!("MessagePack (命名): {} bytes", msgpack_bytes.len());
    println!("MessagePack (数组): {} bytes", msgpack_no_names_bytes.len());
    println!("BSON:             {} bytes", bson_bytes.len());

    println!("\n=== 所有测试通过，使用 cfg 注册系统完美工作 ✓ ===");
    Ok(())
}
