use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::cfg::register_trait;

use super::{
    BsonSerializer, BsonSerializerConfig, JsonSerializer, JsonSerializerConfig, MsgPackSerializer,
    MsgPackSerializerConfig, ProtobufSerializer, ProtobufSerializerConfig, Serializer,
};

/// 注册 Serde 序列化器（Json、MsgPack、Bson）
///
/// 为实现 `Serialize + Deserialize` 的类型注册基础序列化器。
///
/// # 类型参数
/// - `T`: 需要序列化的数据类型，必须实现 Serde traits
pub fn register_serde_serializers<T>() -> Result<()>
where
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    register_trait::<JsonSerializer<T>, dyn Serializer<T, Vec<u8>>, JsonSerializerConfig>(
        "JsonSerializer",
    )?;
    register_trait::<MsgPackSerializer<T>, dyn Serializer<T, Vec<u8>>, MsgPackSerializerConfig>(
        "MsgPackSerializer",
    )?;
    register_trait::<BsonSerializer<T>, dyn Serializer<T, Vec<u8>>, BsonSerializerConfig>(
        "BsonSerializer",
    )?;
    Ok(())
}

/// 注册 Protobuf 序列化器
///
/// 为实现 `prost::Message` 的类型注册 Protobuf 序列化器。
///
/// # 类型参数
/// - `T`: 需要序列化的 protobuf 消息类型
pub fn register_protobuf_serializers<T>() -> Result<()>
where
    T: prost::Message + Default + Send + Sync + 'static,
{
    register_trait::<ProtobufSerializer<T>, dyn Serializer<T, Vec<u8>>, ProtobufSerializerConfig>(
        "ProtobufSerializer",
    )?;
    Ok(())
}

/// 注册所有序列化器（Serde + Protobuf）
///
/// 为同时实现 `Serialize + Deserialize` 和 `prost::Message + Default` 的类型
/// 注册所有四种序列化器：JsonSerializer、MsgPackSerializer、BsonSerializer、ProtobufSerializer
///
/// # 类型参数
/// - `T`: 需要同时满足 Serde 和 Protobuf trait bounds
///
/// # 示例
/// ```ignore
/// use rustx::kv::serializer::register_serializers;
/// use rustx::proto::User;
///
/// // protobuf 类型同时实现了 Serde 和 prost::Message
/// // 会注册所有四种序列化器
/// register_serializers::<User>()?;
/// ```
pub fn register_serializers<T>() -> Result<()>
where
    T: Serialize + for<'de> Deserialize<'de> + prost::Message + Default + Send + Sync + 'static,
{
    register_serde_serializers::<T>()?;
    register_protobuf_serializers::<T>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{create_trait_from_type_options, TypeOptions};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct TestUser {
        name: String,
        age: u32,
        active: bool,
    }

    #[tokio::test]
    async fn test_register_json_serializer() -> Result<()> {
        register_serde_serializers::<TestUser>()?;

        let opts = TypeOptions::from_json(
            r#"{
            "type": "JsonSerializer",
            "options": { "pretty": false }
        }"#,
        )?;

        let serializer: Box<dyn Serializer<TestUser, Vec<u8>>> =
            create_trait_from_type_options(&opts)?;

        let user = TestUser {
            name: "Alice".to_string(),
            age: 30,
            active: true,
        };

        // 验证序列化器可以正常使用
        let bytes = serializer.serialize(user.clone()).await.unwrap();
        let deserialized = serializer.deserialize(bytes).await.unwrap();
        assert_eq!(user, deserialized);

        Ok(())
    }

    #[tokio::test]
    async fn test_register_msgpack_serializer() -> Result<()> {
        register_serde_serializers::<TestUser>()?;

        let opts = TypeOptions::from_json(
            r#"{
            "type": "MsgPackSerializer", 
            "options": { "named": true }
        }"#,
        )?;

        let serializer: Box<dyn Serializer<TestUser, Vec<u8>>> =
            create_trait_from_type_options(&opts)?;

        let user = TestUser {
            name: "Bob".to_string(),
            age: 25,
            active: false,
        };

        let bytes = serializer.serialize(user.clone()).await.unwrap();
        let deserialized = serializer.deserialize(bytes).await.unwrap();
        assert_eq!(user, deserialized);

        Ok(())
    }

    #[tokio::test]
    async fn test_register_bson_serializer() -> Result<()> {
        register_serde_serializers::<TestUser>()?;

        let opts = TypeOptions::from_json(
            r#"{
            "type": "BsonSerializer",
            "options": { "utc": true }
        }"#,
        )?;

        let serializer: Box<dyn Serializer<TestUser, Vec<u8>>> =
            create_trait_from_type_options(&opts)?;

        let user = TestUser {
            name: "Charlie".to_string(),
            age: 35,
            active: true,
        };

        let bytes = serializer.serialize(user.clone()).await.unwrap();
        let deserialized = serializer.deserialize(bytes).await.unwrap();
        assert_eq!(user, deserialized);

        Ok(())
    }

    #[tokio::test]
    async fn test_register_multiple_types() -> Result<()> {
        // 注册多种类型的序列化器
        register_serde_serializers::<TestUser>()?;
        register_serde_serializers::<String>()?;
        register_serde_serializers::<i32>()?;

        // 验证不同类型都能正常工作
        let opts_json = TypeOptions::from_json(r#"{"type": "JsonSerializer", "options": {}}"#)?;
        let opts_msgpack =
            TypeOptions::from_json(r#"{"type": "MsgPackSerializer", "options": {}}"#)?;

        let _serializer1: Box<dyn Serializer<TestUser, Vec<u8>>> =
            create_trait_from_type_options(&opts_json)?;
        let _serializer2: Box<dyn Serializer<String, Vec<u8>>> =
            create_trait_from_type_options(&opts_msgpack)?;

        Ok(())
    }

    #[tokio::test]
    async fn test_json_pretty_option() -> Result<()> {
        register_serde_serializers::<TestUser>()?;

        let opts_pretty = TypeOptions::from_json(
            r#"{
            "type": "JsonSerializer",
            "options": { "pretty": true }
        }"#,
        )?;

        let serializer: Box<dyn Serializer<TestUser, Vec<u8>>> =
            create_trait_from_type_options(&opts_pretty)?;

        let user = TestUser {
            name: "Diana".to_string(),
            age: 28,
            active: true,
        };

        let bytes = serializer.serialize(user.clone()).await.unwrap();
        let json_str = String::from_utf8(bytes.clone()).unwrap();

        // 验证 pretty 格式（包含换行符）
        assert!(json_str.contains('\n'));

        let deserialized = serializer.deserialize(bytes).await.unwrap();
        assert_eq!(user, deserialized);

        Ok(())
    }

    #[tokio::test]
    async fn test_register_protobuf_serializer() -> Result<()> {
        use crate::proto::User;

        // protobuf User 类型实现了 prost::Message，
        // register_serializers 会自动检测并注册 ProtobufSerializer
        register_serializers::<User>()?;

        let opts = TypeOptions::from_json(
            r#"{
            "type": "ProtobufSerializer",
            "options": {}
        }"#,
        )?;

        let serializer: Box<dyn Serializer<User, Vec<u8>>> = create_trait_from_type_options(&opts)?;

        let user = User {
            name: "Alice".to_string(),
            age: 30,
            active: true,
        };

        // 验证序列化器可以正常使用
        let bytes = serializer.serialize(user.clone()).await.unwrap();
        let deserialized = serializer.deserialize(bytes).await.unwrap();

        assert_eq!(user.name, deserialized.name);
        assert_eq!(user.age, deserialized.age);
        assert_eq!(user.active, deserialized.active);

        Ok(())
    }
}
