use crate::kv::serializer::core::{Serializer, SerializerError};
use prost::Message;
use serde::Deserialize;
use std::marker::PhantomData;

/// Protobuf 序列化器配置
#[derive(Deserialize, Debug, Clone)]
pub struct ProtobufSerializerConfig {}

impl Default for ProtobufSerializerConfig {
    fn default() -> Self {
        Self {}
    }
}

/// Protobuf 序列化器
///
/// 支持任意实现了 prost::Message 的类型与字节数组之间的序列化
/// 
/// 注意：这个序列化器与其他序列化器不同，它要求类型 T 必须实现 prost::Message trait
/// 而不是 serde 的 Serialize/Deserialize，因为 protobuf 有自己的序列化机制
pub struct ProtobufSerializer<T> {
    _phantom: PhantomData<T>,
}

impl<T> ProtobufSerializer<T> {
    /// 创建 Protobuf 序列化器的唯一方法
    ///
    /// # 参数
    /// * `_config` - Protobuf 序列化器配置
    pub fn new(_config: ProtobufSerializerConfig) -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<T> Serializer<T, Vec<u8>> for ProtobufSerializer<T>
where
    T: Message + Default + Send + Sync,
{
    fn serialize(&self, from: T) -> Result<Vec<u8>, SerializerError> {
        let mut buf = Vec::new();
        from.encode(&mut buf)
            .map_err(|e| SerializerError::SerializationFailed(e.to_string()))?;
        Ok(buf)
    }

    fn deserialize(&self, to: Vec<u8>) -> Result<T, SerializerError> {
        T::decode(&to[..]).map_err(|e| SerializerError::DeserializationFailed(e.to_string()))
    }
}

/// 支持 cfg 模块类型注册的 From trait 实现
impl<T> From<ProtobufSerializerConfig> for ProtobufSerializer<T> {
    fn from(config: ProtobufSerializerConfig) -> Self {
        ProtobufSerializer::new(config)
    }
}

impl<T> From<Box<ProtobufSerializer<T>>> for Box<dyn Serializer<T, Vec<u8>>>
where
    T: Message + Default + Send + Sync + 'static,
{
    fn from(source: Box<ProtobufSerializer<T>>) -> Self {
        source as Box<dyn Serializer<T, Vec<u8>>>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{User, Product, Order};

    #[test]
    fn test_protobuf_serializer_user() {
        let config = ProtobufSerializerConfig::default();
        let serializer = ProtobufSerializer::new(config);

        let user = User {
            name: "Alice".to_string(),
            age: 30,
            active: true,
        };

        // 序列化
        let bytes = serializer.serialize(user.clone()).unwrap();

        // 反序列化
        let deserialized: User = serializer.deserialize(bytes).unwrap();

        assert_eq!(user.name, deserialized.name);
        assert_eq!(user.age, deserialized.age);
        assert_eq!(user.active, deserialized.active);
    }

    #[test]
    fn test_protobuf_serializer_product() {
        let config = ProtobufSerializerConfig::default();
        let serializer = ProtobufSerializer::new(config);

        let product = Product {
            id: 12345,
            name: "Laptop".to_string(),
            price: 999.99,
            tags: vec!["electronics".to_string(), "computer".to_string()],
        };

        // 序列化
        let bytes = serializer.serialize(product.clone()).unwrap();

        // 反序列化
        let deserialized: Product = serializer.deserialize(bytes).unwrap();

        assert_eq!(product.id, deserialized.id);
        assert_eq!(product.name, deserialized.name);
        assert_eq!(product.price, deserialized.price);
        assert_eq!(product.tags, deserialized.tags);
    }

    #[test]
    fn test_protobuf_serializer_nested() {
        let config = ProtobufSerializerConfig::default();
        let serializer = ProtobufSerializer::new(config);

        let user = User {
            name: "Bob".to_string(),
            age: 25,
            active: true,
        };

        let product = Product {
            id: 67890,
            name: "Phone".to_string(),
            price: 599.99,
            tags: vec!["mobile".to_string()],
        };

        let order = Order {
            id: 1001,
            user: Some(user.clone()),
            products: vec![product.clone()],
            total_amount: 599.99,
            created_at: 1634567890,
        };

        // 序列化
        let bytes = serializer.serialize(order.clone()).unwrap();

        // 反序列化
        let deserialized: Order = serializer.deserialize(bytes).unwrap();

        assert_eq!(order.id, deserialized.id);
        assert_eq!(order.total_amount, deserialized.total_amount);
        assert_eq!(order.created_at, deserialized.created_at);

        // 验证嵌套的用户信息
        let des_user = deserialized.user.unwrap();
        assert_eq!(user.name, des_user.name);
        assert_eq!(user.age, des_user.age);
        assert_eq!(user.active, des_user.active);

        // 验证产品列表
        assert_eq!(order.products.len(), deserialized.products.len());
        let des_product = &deserialized.products[0];
        assert_eq!(product.id, des_product.id);
        assert_eq!(product.name, des_product.name);
        assert_eq!(product.price, des_product.price);
    }
}