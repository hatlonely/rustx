-- Test data initialization script
-- Create test application and configurations for Rust tests

USE ApolloConfigDB;

-- Create test application
INSERT INTO `App` (`AppId`, `Name`, `OrgId`, `OrgName`, `OwnerName`, `OwnerEmail`, `DataChange_CreatedBy`)
VALUES ('test-app', 'Test Application', 'TEST', 'Test Department', 'apollo', 'apollo@admin.com', 'apollo');

-- Create default cluster
INSERT INTO `Cluster` (`Name`, `AppId`, `DataChange_CreatedBy`)
VALUES ('default', 'test-app', 'apollo');

-- Create application namespace
INSERT INTO `AppNamespace` (`Name`, `AppId`, `Format`, `IsPublic`, `Comment`, `DataChange_CreatedBy`)
VALUES ('application', 'test-app', 'properties', 0, 'Default namespace', 'apollo');

-- Create namespace instance
INSERT INTO `Namespace` (`AppId`, `ClusterName`, `NamespaceName`, `DataChange_CreatedBy`)
VALUES ('test-app', 'default', 'application', 'apollo');

-- Add test configuration items
-- database config (JSON format, TypeOptions structure: {type, options})
INSERT INTO `Item` (`NamespaceId`, `Key`, `Value`, `Comment`, `LineNum`, `DataChange_CreatedBy`)
VALUES (
    (SELECT Id FROM `Namespace` WHERE AppId = 'test-app' AND ClusterName = 'default' AND NamespaceName = 'application'),
    'database',
    '{"type":"DatabaseService","options":{"host":"localhost","port":3306,"username":"root","password":"secret","database":"test_db","max_connections":10}}',
    'Database configuration',
    1,
    'apollo'
);

-- redis config
INSERT INTO `Item` (`NamespaceId`, `Key`, `Value`, `Comment`, `LineNum`, `DataChange_CreatedBy`)
VALUES (
    (SELECT Id FROM `Namespace` WHERE AppId = 'test-app' AND ClusterName = 'default' AND NamespaceName = 'application'),
    'redis',
    '{"type":"RedisService","options":{"host":"localhost","port":6379,"password":"","database":0}}',
    'Redis configuration',
    2,
    'apollo'
);

-- app config (simple key-value pairs)
INSERT INTO `Item` (`NamespaceId`, `Key`, `Value`, `Comment`, `LineNum`, `DataChange_CreatedBy`)
VALUES (
    (SELECT Id FROM `Namespace` WHERE AppId = 'test-app' AND ClusterName = 'default' AND NamespaceName = 'application'),
    'app.name',
    'RustX Test Application',
    'Application name',
    3,
    'apollo'
);

INSERT INTO `Item` (`NamespaceId`, `Key`, `Value`, `Comment`, `LineNum`, `DataChange_CreatedBy`)
VALUES (
    (SELECT Id FROM `Namespace` WHERE AppId = 'test-app' AND ClusterName = 'default' AND NamespaceName = 'application'),
    'app.version',
    '1.0.0',
    'Application version',
    4,
    'apollo'
);

-- Create initial release
INSERT INTO `Release` (`ReleaseKey`, `Name`, `Comment`, `AppId`, `ClusterName`, `NamespaceName`, `Configurations`, `DataChange_CreatedBy`)
VALUES (
    CONCAT('20240101000000-', UUID()),
    'Initial Release',
    'Initialize test configurations',
    'test-app',
    'default',
    'application',
    '{"database":"{\\\"type\\\":\\\"DatabaseService\\\",\\\"options\\\":{\\\"host\\\":\\\"localhost\\\",\\\"port\\\":3306,\\\"username\\\":\\\"root\\\",\\\"password\\\":\\\"secret\\\",\\\"database\\\":\\\"test_db\\\",\\\"max_connections\\\":10}}","redis":"{\\\"type\\\":\\\"RedisService\\\",\\\"options\\\":{\\\"host\\\":\\\"localhost\\\",\\\"port\\\":6379,\\\"password\\\":\\\"\\\",\\\"database\\\":0}}","app.name":"RustX Test Application","app.version":"1.0.0"}',
    'apollo'
);

-- Sync to Portal DB
USE ApolloPortalDB;

INSERT INTO `App` (`AppId`, `Name`, `OrgId`, `OrgName`, `OwnerName`, `OwnerEmail`, `DataChange_CreatedBy`)
VALUES ('test-app', 'Test Application', 'TEST', 'Test Department', 'apollo', 'apollo@admin.com', 'apollo');

INSERT INTO `AppNamespace` (`Name`, `AppId`, `Format`, `IsPublic`, `Comment`, `DataChange_CreatedBy`)
VALUES ('application', 'test-app', 'properties', 0, 'Default namespace', 'apollo');

-- Create necessary Permissions for test-app
INSERT INTO `Permission` (`PermissionType`, `TargetId`, `DataChange_CreatedBy`) VALUES
('ModifyNamespace', 'test-app+application', 'apollo'),
('ReleaseNamespace', 'test-app+application', 'apollo'),
('ModifyNamespace', 'test-app+application+DEV', 'apollo'),
('ReleaseNamespace', 'test-app+application+DEV', 'apollo'),
('CreateCluster', 'test-app', 'apollo'),
('CreateNamespace', 'test-app', 'apollo'),
('AssignRole', 'test-app', 'apollo');

-- Create necessary Roles for test-app
INSERT INTO `Role` (`RoleName`, `DataChange_CreatedBy`) VALUES
('Master+test-app', 'apollo'),
('ModifyNamespace+test-app+application', 'apollo'),
('ReleaseNamespace+test-app+application', 'apollo'),
('ModifyNamespace+test-app+application+DEV', 'apollo'),
('ReleaseNamespace+test-app+application+DEV', 'apollo');

-- Bind Role and Permission
INSERT INTO `RolePermission` (`RoleId`, `PermissionId`, `DataChange_CreatedBy`)
SELECT r.Id, p.Id, 'apollo'
FROM `Role` r, `Permission` p
WHERE (r.RoleName = 'ModifyNamespace+test-app+application' AND p.PermissionType = 'ModifyNamespace' AND p.TargetId = 'test-app+application')
   OR (r.RoleName = 'ReleaseNamespace+test-app+application' AND p.PermissionType = 'ReleaseNamespace' AND p.TargetId = 'test-app+application')
   OR (r.RoleName = 'ModifyNamespace+test-app+application+DEV' AND p.PermissionType = 'ModifyNamespace' AND p.TargetId = 'test-app+application+DEV')
   OR (r.RoleName = 'ReleaseNamespace+test-app+application+DEV' AND p.PermissionType = 'ReleaseNamespace' AND p.TargetId = 'test-app+application+DEV');

-- Set apollo user as Master of test-app
INSERT INTO `UserRole` (`UserId`, `RoleId`, `DataChange_CreatedBy`)
SELECT 'apollo', Id, 'apollo' FROM `Role` WHERE `RoleName` = 'Master+test-app';

-- Grant apollo user modify and release permissions
INSERT INTO `UserRole` (`UserId`, `RoleId`, `DataChange_CreatedBy`)
SELECT 'apollo', Id, 'apollo' FROM `Role` WHERE `RoleName` IN (
    'ModifyNamespace+test-app+application',
    'ReleaseNamespace+test-app+application',
    'ModifyNamespace+test-app+application+DEV',
    'ReleaseNamespace+test-app+application+DEV'
);

-- Create OpenAPI Consumer and Token (for functional tests to modify config dynamically)
INSERT INTO `Consumer` (`Name`, `AppId`, `OrgId`, `OrgName`, `OwnerName`, `OwnerEmail`, `DataChange_CreatedBy`)
VALUES ('rustx-test-consumer', 'test-app', 'TEST', 'Test Department', 'apollo', 'apollo@admin.com', 'apollo');

-- Create fixed test Token (use secure random token in production)
INSERT INTO `ConsumerToken` (`ConsumerId`, `Token`, `DataChange_CreatedBy`)
VALUES (
    (SELECT Id FROM `Consumer` WHERE `Name` = 'rustx-test-consumer'),
    'rustx-test-token-20240101',
    'apollo'
);

-- Grant Consumer access to test-app
INSERT INTO `ConsumerRole` (`ConsumerId`, `RoleId`, `DataChange_CreatedBy`)
VALUES (
    (SELECT Id FROM `Consumer` WHERE `Name` = 'rustx-test-consumer'),
    (SELECT Id FROM `Role` WHERE `RoleName` = 'ModifyNamespace+test-app+application'),
    'apollo'
);

-- Grant release permission
INSERT INTO `ConsumerRole` (`ConsumerId`, `RoleId`, `DataChange_CreatedBy`)
VALUES (
    (SELECT Id FROM `Consumer` WHERE `Name` = 'rustx-test-consumer'),
    (SELECT Id FROM `Role` WHERE `RoleName` = 'ReleaseNamespace+test-app+application'),
    'apollo'
);
