-- 测试数据初始化脚本
-- 创建测试应用和配置供 Rust 测试使用

USE ApolloConfigDB;

-- 创建测试应用
INSERT INTO `App` (`AppId`, `Name`, `OrgId`, `OrgName`, `OwnerName`, `OwnerEmail`, `DataChange_CreatedBy`)
VALUES ('test-app', '测试应用', 'TEST', '测试部门', 'apollo', 'apollo@admin.com', 'apollo');

-- 创建 default 集群
INSERT INTO `Cluster` (`Name`, `AppId`, `DataChange_CreatedBy`)
VALUES ('default', 'test-app', 'apollo');

-- 创建 application namespace
INSERT INTO `AppNamespace` (`Name`, `AppId`, `Format`, `IsPublic`, `Comment`, `DataChange_CreatedBy`)
VALUES ('application', 'test-app', 'properties', 0, '默认命名空间', 'apollo');

-- 创建 namespace
INSERT INTO `Namespace` (`AppId`, `ClusterName`, `NamespaceName`, `DataChange_CreatedBy`)
VALUES ('test-app', 'default', 'application', 'apollo');

-- 添加测试配置项
-- database 配置 (JSON 格式的配置，符合 TypeOptions 结构: {type, options})
INSERT INTO `Item` (`NamespaceId`, `Key`, `Value`, `Comment`, `LineNum`, `DataChange_CreatedBy`)
VALUES (
    (SELECT Id FROM `Namespace` WHERE AppId = 'test-app' AND ClusterName = 'default' AND NamespaceName = 'application'),
    'database',
    '{"type":"DatabaseService","options":{"host":"localhost","port":3306,"username":"root","password":"secret","database":"test_db","max_connections":10}}',
    '数据库配置',
    1,
    'apollo'
);

-- redis 配置
INSERT INTO `Item` (`NamespaceId`, `Key`, `Value`, `Comment`, `LineNum`, `DataChange_CreatedBy`)
VALUES (
    (SELECT Id FROM `Namespace` WHERE AppId = 'test-app' AND ClusterName = 'default' AND NamespaceName = 'application'),
    'redis',
    '{"type":"RedisService","options":{"host":"localhost","port":6379,"password":"","database":0}}',
    'Redis配置',
    2,
    'apollo'
);

-- app 配置 (简单键值对)
INSERT INTO `Item` (`NamespaceId`, `Key`, `Value`, `Comment`, `LineNum`, `DataChange_CreatedBy`)
VALUES (
    (SELECT Id FROM `Namespace` WHERE AppId = 'test-app' AND ClusterName = 'default' AND NamespaceName = 'application'),
    'app.name',
    'RustX Test Application',
    '应用名称',
    3,
    'apollo'
);

INSERT INTO `Item` (`NamespaceId`, `Key`, `Value`, `Comment`, `LineNum`, `DataChange_CreatedBy`)
VALUES (
    (SELECT Id FROM `Namespace` WHERE AppId = 'test-app' AND ClusterName = 'default' AND NamespaceName = 'application'),
    'app.version',
    '1.0.0',
    '应用版本',
    4,
    'apollo'
);

-- 创建初始发布
INSERT INTO `Release` (`ReleaseKey`, `Name`, `Comment`, `AppId`, `ClusterName`, `NamespaceName`, `Configurations`, `DataChange_CreatedBy`)
VALUES (
    CONCAT('20240101000000-', UUID()),
    '初始发布',
    '初始化测试配置',
    'test-app',
    'default',
    'application',
    '{"database":"{\\\"type\\\":\\\"DatabaseService\\\",\\\"options\\\":{\\\"host\\\":\\\"localhost\\\",\\\"port\\\":3306,\\\"username\\\":\\\"root\\\",\\\"password\\\":\\\"secret\\\",\\\"database\\\":\\\"test_db\\\",\\\"max_connections\\\":10}}","redis":"{\\\"type\\\":\\\"RedisService\\\",\\\"options\\\":{\\\"host\\\":\\\"localhost\\\",\\\"port\\\":6379,\\\"password\\\":\\\"\\\",\\\"database\\\":0}}","app.name":"RustX Test Application","app.version":"1.0.0"}',
    'apollo'
);

-- 同步到 Portal DB
USE ApolloPortalDB;

INSERT INTO `App` (`AppId`, `Name`, `OrgId`, `OrgName`, `OwnerName`, `OwnerEmail`, `DataChange_CreatedBy`)
VALUES ('test-app', '测试应用', 'TEST', '测试部门', 'apollo', 'apollo@admin.com', 'apollo');

INSERT INTO `AppNamespace` (`Name`, `AppId`, `Format`, `IsPublic`, `Comment`, `DataChange_CreatedBy`)
VALUES ('application', 'test-app', 'properties', 0, '默认命名空间', 'apollo');

-- 创建 OpenAPI Consumer 和 Token (用于功能测试动态修改配置)
INSERT INTO `Consumer` (`Name`, `AppId`, `OrgId`, `OrgName`, `OwnerName`, `OwnerEmail`, `DataChange_CreatedBy`)
VALUES ('rustx-test-consumer', 'test-app', 'TEST', '测试部门', 'apollo', 'apollo@admin.com', 'apollo');

-- 创建固定的测试 Token (实际生产环境请使用安全的随机 token)
INSERT INTO `ConsumerToken` (`ConsumerId`, `Token`, `DataChange_CreatedBy`)
VALUES (
    (SELECT Id FROM `Consumer` WHERE `Name` = 'rustx-test-consumer'),
    'rustx-test-token-20240101',
    'apollo'
);

-- 授权 Consumer 访问 test-app
INSERT INTO `ConsumerRole` (`ConsumerId`, `RoleId`, `DataChange_CreatedBy`)
VALUES (
    (SELECT Id FROM `Consumer` WHERE `Name` = 'rustx-test-consumer'),
    (SELECT Id FROM `Role` WHERE `RoleName` = 'ModifyNamespace+test-app+application'),
    'apollo'
);

-- 授权发布权限
INSERT INTO `ConsumerRole` (`ConsumerId`, `RoleId`, `DataChange_CreatedBy`)
VALUES (
    (SELECT Id FROM `Consumer` WHERE `Name` = 'rustx-test-consumer'),
    (SELECT Id FROM `Role` WHERE `RoleName` = 'ReleaseNamespace+test-app+application'),
    'apollo'
);
