use anyhow::Result;
use rustx::cfg::{create_trait_from_type_options, TypeOptions};
use rustx::kv::serializer::{register_protobuf_serializers, Serializer};
use rustx::proto::{Order, Product, User};

fn main() -> Result<()> {
    println!("=== Protobuf 序列化器使用示例 ===");

    // 注册 protobuf 序列化器
    register_protobuf_serializers::<User>()?;
    register_protobuf_serializers::<Product>()?;
    register_protobuf_serializers::<Order>()?;

    // 1. 测试用户消息序列化
    println!("\n1. 用户消息序列化测试");
    test_user_serialization()?;

    // 2. 测试产品消息序列化
    println!("\n2. 产品消息序列化测试");
    test_product_serialization()?;

    // 3. 测试嵌套消息序列化
    println!("\n3. 嵌套消息（订单）序列化测试");
    test_order_serialization()?;

    println!("\n=== 示例完成 ===");
    Ok(())
}

fn test_user_serialization() -> Result<()> {
    // 通过配置创建序列化器
    let opts = TypeOptions::from_json(
        r#"{
        "type": "ProtobufSerializer",
        "options": {}
    }"#,
    )?;

    let serializer: Box<dyn Serializer<User, Vec<u8>>> = create_trait_from_type_options(&opts)?;

    // 创建测试数据
    let user = User {
        name: "Alice".to_string(),
        age: 30,
        active: true,
    };

    println!("原始用户数据: {:?}", user);

    // 序列化
    let bytes = serializer.serialize(user.clone())?;
    println!("序列化后字节长度: {}", bytes.len());

    // 反序列化
    let deserialized: User = serializer.deserialize(bytes)?;
    println!("反序列化后数据: {:?}", deserialized);

    // 验证数据一致性
    assert_eq!(user.name, deserialized.name);
    assert_eq!(user.age, deserialized.age);
    assert_eq!(user.active, deserialized.active);

    println!("✅ 用户序列化测试通过");
    Ok(())
}

fn test_product_serialization() -> Result<()> {
    let opts = TypeOptions::from_json(
        r#"{
        "type": "ProtobufSerializer", 
        "options": {}
    }"#,
    )?;

    let serializer: Box<dyn Serializer<Product, Vec<u8>>> = create_trait_from_type_options(&opts)?;

    let product = Product {
        id: 12345,
        name: "MacBook Pro".to_string(),
        price: 2499.99,
        tags: vec![
            "laptop".to_string(),
            "apple".to_string(),
            "professional".to_string(),
        ],
    };

    println!("原始产品数据: {:?}", product);

    let bytes = serializer.serialize(product.clone())?;
    println!("序列化后字节长度: {}", bytes.len());

    let deserialized: Product = serializer.deserialize(bytes)?;
    println!("反序列化后数据: {:?}", deserialized);

    assert_eq!(product.id, deserialized.id);
    assert_eq!(product.name, deserialized.name);
    assert_eq!(product.price, deserialized.price);
    assert_eq!(product.tags, deserialized.tags);

    println!("✅ 产品序列化测试通过");
    Ok(())
}

fn test_order_serialization() -> Result<()> {
    let opts = TypeOptions::from_json(
        r#"{
        "type": "ProtobufSerializer",
        "options": {}
    }"#,
    )?;

    let serializer: Box<dyn Serializer<Order, Vec<u8>>> = create_trait_from_type_options(&opts)?;

    // 创建复杂的嵌套数据
    let user = User {
        name: "Bob".to_string(),
        age: 25,
        active: true,
    };

    let products = vec![
        Product {
            id: 1001,
            name: "iPhone 15".to_string(),
            price: 999.99,
            tags: vec!["phone".to_string(), "apple".to_string()],
        },
        Product {
            id: 1002,
            name: "AirPods Pro".to_string(),
            price: 249.99,
            tags: vec!["earphones".to_string(), "wireless".to_string()],
        },
    ];

    let order = Order {
        id: 2001,
        user: Some(user.clone()),
        products: products.clone(),
        total_amount: 1249.98,
        created_at: 1703145600, // 2023-12-21
    };

    println!("原始订单数据: {:?}", order);

    let bytes = serializer.serialize(order.clone())?;
    println!("序列化后字节长度: {}", bytes.len());

    let deserialized: Order = serializer.deserialize(bytes)?;
    println!("反序列化后数据: {:?}", deserialized);

    // 验证嵌套数据
    assert_eq!(order.id, deserialized.id);
    assert_eq!(order.total_amount, deserialized.total_amount);
    assert_eq!(order.created_at, deserialized.created_at);

    let des_user = deserialized.user.unwrap();
    assert_eq!(user.name, des_user.name);
    assert_eq!(user.age, des_user.age);
    assert_eq!(user.active, des_user.active);

    assert_eq!(products.len(), deserialized.products.len());
    for (original, des) in products.iter().zip(deserialized.products.iter()) {
        assert_eq!(original.id, des.id);
        assert_eq!(original.name, des.name);
        assert_eq!(original.price, des.price);
        assert_eq!(original.tags, des.tags);
    }

    println!("✅ 订单序列化测试通过");
    Ok(())
}
