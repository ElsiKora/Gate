//! `nestjs/asset-storage` against synthetic modules, adapters and infrastructure.

mod support;

use support::{Row, check, error, row};

const RULE: &str = "asset-storage";
const ANCHOR: &str = "src/modules/internal/public-asset/controller.ts";
const ADAPTER: &str = "src/infrastructure/public-asset/s3/adapter.ts";
const CONSTANT: &str = "src/infrastructure/public-asset/constant/s3.constant.ts";
const ENTITY: &str = "src/modules/domain/user/entity/user.entity.ts";

fn finding(aspect: &str, path: &str, line: Option<u32>, message: &str) -> Row {
    row(aspect, path, line, message)
}

const CLEAN_ADAPTER: &str = "import { ChecksumAlgorithm, ChecksumMode } from \"@aws-sdk/client-s3\";\n\
                             import { HttpStatus } from \"@nestjs/common\";\n\
                             import { S3_CONSTANT } from \"../constant/s3.constant\";\n\
                             export class Adapter {\n\
                             \x20 statuses = [HttpStatus.CONFLICT, HttpStatus.NOT_FOUND, HttpStatus.PRECONDITION_FAILED];\n\
                             \x20 put() { return { ChecksumAlgorithm: ChecksumAlgorithm.SHA256, ChecksumMode: ChecksumMode.ENABLED, bucket: S3_CONSTANT.BUCKET }; }\n\
                             }\n";

#[test]
fn keys_infrastructure_sdks_and_a_conventional_adapter_pass() {
    let entity = "import { Column, Entity } from \"typeorm\";\n\
                  @Entity()\n\
                  export class UserEntity {\n\
                  \x20 @Column() avatarKey!: string;\n\
                  \x20 avatar?: string;\n\
                  }\n";
    let files = [
        (ENTITY, entity),
        (ANCHOR, "export class Controller {}\n"),
        (ADAPTER, CLEAN_ADAPTER),
        (CONSTANT, "export const S3_CONSTANT = { BUCKET: \"b\" };\n"),
        (
            "src/modules/internal/public-asset/create.ts",
            "export const contentBase64 = \"\";\n",
        ),
    ];

    assert_eq!(check(RULE, "{}", &files), []);
}

#[test]
fn image_fields_and_sdk_imports_are_checked_without_central_admission() {
    let entity = "import * as orm from \"typeorm\";\n\
                  import { Column as Field } from \"typeorm\";\n\
                  export class UserEntity {\n\
                  \x20 @orm.Column() avatar!: string;\n\
                  \x20 @Field()\n\
                  \x20 bannerUrl!: string;\n\
                  }\n";
    let runtime = "import { CloudFrontClient } from \"@aws-sdk/client-cloudfront\";\n\
                   import { S3Client } from \"@aws-sdk/client-s3/dist\";\n";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                (ENTITY, entity),
                ("src/modules/public/user/user.service.ts", runtime),
                ("src/infrastructure/storage/client.ts", runtime),
            ]
        ),
        [
            finding(
                "persisted-image-key",
                ENTITY,
                Some(4),
                &format!(
                    "{ENTITY} persists image-like field avatar; persisted image references must \
                     end in Key"
                )
            ),
            finding(
                "persisted-image-key",
                ENTITY,
                Some(5),
                &format!(
                    "{ENTITY} persists image-like field bannerUrl; persisted image references \
                     must end in Key"
                )
            ),
            finding(
                "cloudfront-cdk-only",
                "src/infrastructure/storage/client.ts",
                None,
                "src/infrastructure/storage/client.ts imports CloudFront at runtime; CloudFront \
                 is CDK-only"
            ),
            finding(
                "cloudfront-cdk-only",
                "src/modules/public/user/user.service.ts",
                None,
                "src/modules/public/user/user.service.ts imports CloudFront at runtime; \
                 CloudFront is CDK-only"
            ),
            finding(
                "s3-sdk-placement",
                "src/modules/public/user/user.service.ts",
                None,
                "src/modules/public/user/user.service.ts imports the S3 SDK outside \
                 infrastructure"
            ),
        ]
    );
}

#[test]
fn central_admission_requires_the_adapter() {
    assert_eq!(
        check(RULE, "{}", &[(ANCHOR, "export class Controller {}\n")]),
        [finding(
            "s3-adapter-required",
            ADAPTER,
            None,
            &format!("{ADAPTER} is required once central PublicAsset admission exists")
        )]
    );
}

#[test]
fn the_adapter_keeps_its_constants_and_enum_members() {
    let adapter = "import { HttpStatus } from \"@nestjs/common\";\n\
                   const LIMIT = 5;\n\
                   const HTTP_STATUS_GONE = 410;\n\
                   export const put = () => ({\n\
                   \x20 ChecksumAlgorithm: \"SHA256\",\n\
                   \x20 status: HttpStatus.CONFLICT,\n\
                   });\n";
    let enums = |member: &str| {
        finding(
            "s3-sdk-enum",
            ADAPTER,
            None,
            &format!("{ADAPTER} must use {member}"),
        )
    };

    assert_eq!(
        check(
            RULE,
            "{}",
            &[(ANCHOR, "export class Controller {}\n"), (ADAPTER, adapter)]
        ),
        [
            finding(
                "s3-policy-constant",
                CONSTANT,
                None,
                &format!("{CONSTANT} must own public-asset S3 policy constants")
            ),
            finding(
                "s3-policy-constant",
                ADAPTER,
                None,
                &format!(
                    "{ADAPTER} declares top-level UPPER_CASE const LIMIT; public-asset S3 policy \
                     constants belong in {CONSTANT}"
                )
            ),
            finding(
                "s3-policy-constant",
                ADAPTER,
                None,
                &format!(
                    "{ADAPTER} declares top-level UPPER_CASE const HTTP_STATUS_GONE; public-asset \
                     S3 policy constants belong in {CONSTANT}"
                )
            ),
            finding(
                "s3-sdk-enum",
                ADAPTER,
                Some(5),
                &format!(
                    "{ADAPTER} uses literal ChecksumAlgorithm: \"SHA256\"; use \
                     ChecksumAlgorithm.SHA256"
                )
            ),
            finding(
                "s3-sdk-enum",
                ADAPTER,
                None,
                &format!("{ADAPTER} declares HTTP_STATUS_GONE; use HttpStatus enum members")
            ),
            enums("HttpStatus.NOT_FOUND"),
            enums("HttpStatus.PRECONDITION_FAILED"),
            enums("ChecksumAlgorithm.SHA256"),
            enums("ChecksumMode.ENABLED"),
        ]
    );
}

#[test]
fn feature_modules_admit_and_store_no_assets() {
    let feature = "export class UserController {\n\
                   \x20 upload(contentBase64: string) { return \"users/:id/image\"; }\n\
                   \x20 setAvatarImage(): void {}\n\
                   }\n";
    let domain = "import { IPublicAssetStorage } from \"@modules/domain/public-asset\";\n\
                  export class UserService { replaceAsset(storage: IPublicAssetStorage): void {} }\n";
    let public = "src/modules/public/user/user.controller.ts";
    let business = "src/modules/domain/user/user.service.ts";
    let files = [
        (ANCHOR, "export class Controller {}\n"),
        (ADAPTER, CLEAN_ADAPTER),
        (CONSTANT, "export const S3_CONSTANT = { BUCKET: \"b\" };\n"),
        (public, feature),
        (business, domain),
        ("src/modules/domain/public-asset/storage.ts", domain),
        (
            "src/modules/domain/user/notes.ts",
            "// IPublicAssetStorageX is no access\n",
        ),
    ];

    assert_eq!(
        check(RULE, "{}", &files),
        [
            finding(
                "central-admission",
                public,
                None,
                &format!(
                    "{public} admits public-asset bytes outside the central internal PublicAsset \
                     resource"
                )
            ),
            finding(
                "central-admission",
                public,
                None,
                &format!(
                    "{public} declares a feature image route outside central PublicAsset CREATE"
                )
            ),
            finding(
                "central-admission",
                public,
                None,
                &format!(
                    "{public} declares a feature asset command outside central PublicAsset CREATE"
                )
            ),
            finding(
                "central-admission",
                business,
                None,
                &format!("{business} declares a feature asset command in a business domain")
            ),
            finding(
                "domain-storage-access",
                business,
                None,
                &format!("{business} consumes public-asset storage from a business domain")
            ),
        ]
    );
}

#[test]
fn paths_patterns_and_members_are_options() {
    let options = r#"{
        "central-anchor": "src/asset.ts",
        "s3-adapter": "src/infrastructure/s3.ts",
        "s3-policy-constant": "src/infrastructure/s3.constant.ts",
        "image-key-suffix": "Ref$",
        "required-members": [],
        "forbidden-literals": [["StorageClass", "STANDARD"]]
    }"#;
    let entity = "import { Column } from \"typeorm\";\n\
                  export class UserEntity { @Column() avatarRef!: string; @Column() logoKey!: string; }\n";
    let adapter = "export const put = () => ({ StorageClass: \"STANDARD\" });\n";

    assert_eq!(
        check(
            RULE,
            options,
            &[
                (ENTITY, entity),
                ("src/asset.ts", "export {};\n"),
                ("src/infrastructure/s3.ts", adapter),
                (
                    "src/infrastructure/s3.constant.ts",
                    "export const S3 = {};\n"
                ),
            ]
        ),
        [
            finding(
                "persisted-image-key",
                ENTITY,
                Some(2),
                &format!(
                    "{ENTITY} persists image-like field logoKey; persisted image references must \
                     end in Key"
                )
            ),
            finding(
                "s3-sdk-enum",
                "src/infrastructure/s3.ts",
                Some(1),
                "src/infrastructure/s3.ts uses literal StorageClass: \"STANDARD\"; use \
                 StorageClass.STANDARD"
            ),
        ]
    );

    let message = error(RULE, r#"{ "image-route-pattern": "(" }"#, &[]);
    assert!(message.contains("image-route-pattern"), "{message}");
}

#[test]
fn the_walk_reads_files_in_skipped_folders() {
    let runtime = "import { CloudFrontClient } from \"@aws-sdk/client-cloudfront\";\n";
    let built = "src/modules/dist/user/user.service.ts";
    let generated = "src/modules/generated/user/user.service.ts";
    let files = [(built, runtime), (generated, runtime)];
    let paths = |languages: &str| support::paths_with(RULE, "{}", "{}", languages, &files);

    assert_eq!(paths("{}"), [built, generated]);
    assert_eq!(paths(support::SKIP_GENERATED), [built, generated]);
}
