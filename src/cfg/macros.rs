//! 配置管理相关的宏定义
//!
//! 提供简化 From trait 实现的宏

/// 为配置类型自动实现 From trait
/// 
/// 支持三种模式：
/// 1. `impl_from!(ConfigType => Type)` - 调用 Type::new(config)
/// 2. `impl_from!(ConfigType => Type, expect: "错误消息")` - 调用 Type::new(config).expect("错误消息")
/// 3. `impl_from!(ConfigType => Type, field: config)` - 使用 Self { config }
#[macro_export]
macro_rules! impl_from {
    // 模式1: 直接调用 new 方法
    ($config_type:ty => $target_type:ty) => {
        impl From<$config_type> for $target_type {
            fn from(config: $config_type) -> Self {
                <$target_type>::new(config)
            }
        }
    };
    
    // 模式2: 调用可能失败的 new 方法，使用 expect
    ($config_type:ty => $target_type:ty, expect: $msg:literal) => {
        impl From<$config_type> for $target_type {
            fn from(config: $config_type) -> Self {
                <$target_type>::new(config).expect($msg)
            }
        }
    };
    
    // 模式3: 直接使用配置字段构造
    ($config_type:ty => $target_type:ty, field: $field:ident) => {
        impl From<$config_type> for $target_type {
            fn from(config: $config_type) -> Self {
                Self { $field: config }
            }
        }
    };
}

/// 为 Box<T> 类型自动实现到 Box<dyn Trait> 的转换
/// 
/// 用法：`impl_box_from!(Type => dyn TraitName)`
#[macro_export]
macro_rules! impl_box_from {
    ($source_type:ty => dyn $trait_name:path) => {
        impl From<Box<$source_type>> for Box<dyn $trait_name> {
            fn from(source: Box<$source_type>) -> Self {
                source as Box<dyn $trait_name>
            }
        }
    };
}

#[cfg(test)]
mod tests {

    #[derive(Debug)]
    struct TestConfig {
        value: String,
    }

    #[derive(Debug)]
    struct TestService {
        config: TestConfig,
    }

    impl TestService {
        fn new(config: TestConfig) -> Self {
            Self { config }
        }
    }

    #[derive(Debug)]
    struct TestServiceWithField {
        config: TestConfig,
    }

    trait TestTrait {
        fn get_value(&self) -> &str;
    }

    impl TestTrait for TestService {
        fn get_value(&self) -> &str {
            &self.config.value
        }
    }

    // 测试三种不同的宏用法
    impl_from!(TestConfig => TestService);
    impl_from!(TestConfig => TestServiceWithField, field: config);
    impl_box_from!(TestService => dyn TestTrait);

    #[test]
    fn test_impl_from_new() {
        let config = TestConfig {
            value: "test".to_string(),
        };
        let service = TestService::from(config);
        assert_eq!(service.config.value, "test");
    }

    #[test]
    fn test_impl_from_field() {
        let config = TestConfig {
            value: "test".to_string(),
        };
        let service = TestServiceWithField::from(config);
        assert_eq!(service.config.value, "test");
    }

    #[test]
    fn test_impl_box_from() {
        let config = TestConfig {
            value: "test".to_string(),
        };
        let service = TestService::from(config);
        let boxed_service = Box::new(service);
        let boxed_trait: Box<dyn TestTrait> = boxed_service.into();
        assert_eq!(boxed_trait.get_value(), "test");
    }
}