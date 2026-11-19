//! `nestjs/notification-egress` against synthetic owners, a transport and a consumer port.

mod support;

use support::{Row, check, error, row, rows, run_from};

const RULE: &str = "notification-egress";
const OPTIONS_FILE: &str = "gate/options/nestjs/notification-egress.json";

const OPTIONS: &str = r#"{
  "owner-classes": ["NOTIFICATION", "PAYMENT"],
  "notification-class": "NOTIFICATION",
  "consumer": { "name": "DeliveryConsumer", "path": "src/common/delivery/consumer.ts" },
  "consumer-label": "delivery consumer",
  "egress": {
    "modules": ["@aws-sdk/client-sesv2", "node:https"],
    "families": ["@aws-sdk/client-"],
    "platform": [{ "module": "@aws-sdk/client-ssm", "decisions": ["D-0004"] }],
    "excluded": [{ "module": "node:net", "reason": "isIP only" }],
    "transport": {
      "root": "src/infrastructure/transport/",
      "modules": ["node:https"],
      "symbols": ["HttpTransport"],
      "decision": "D-0003"
    }
  },
  "owners": [
    {
      "root": "src/infrastructure/mail/",
      "classes": ["NOTIFICATION"],
      "decision": "D-0001",
      "senders": [{ "path": "src/modules/admin/notify.ts", "decision": "D-0001" }]
    },
    { "root": "src/infrastructure/payment/", "classes": ["PAYMENT"], "decision": "D-0002" }
  ]
}"#;

const CONSUMER: (&str, &str) = (
    "src/common/delivery/consumer.ts",
    "export abstract class DeliveryConsumer {}\n",
);
const PORT: (&str, &str) = (
    "src/common/port/mail.port.ts",
    "export interface MailPort { send(): void; }\n",
);
const MAIL: (&str, &str) = (
    "src/infrastructure/mail/client.ts",
    "import { SESv2Client } from \"@aws-sdk/client-sesv2\";\n\
     import { MailPort } from \"../../common/port/mail.port\";\n\
     export class MailClient implements MailPort { send(): void { new SESv2Client({}); } }\n",
);
const MAIL_BARREL: (&str, &str) = (
    "src/infrastructure/mail/index.ts",
    "export { MailClient } from \"./client\";\n",
);
const TRANSPORT: (&str, &str) = (
    "src/infrastructure/transport/http.ts",
    "import { request } from \"node:https\";\n\
     export class HttpTransport { send() { return request; } }\n",
);
const PAYMENT: (&str, &str) = (
    "src/infrastructure/payment/gateway.ts",
    "import { HttpTransport } from \"../transport/http\";\n\
     export const pay = () => fetch(\"https://example.test\");\n\
     export const transport = new HttpTransport();\n",
);
const PARAMETERS: (&str, &str) = (
    "src/config/parameters.ts",
    "import { SSMClient } from \"@aws-sdk/client-ssm\";\n\
     import { isIP } from \"node:net\";\n\
     export const client = new SSMClient({}); export const ip = isIP;\n",
);
const DELIVERY: (&str, &str) = (
    "src/modules/user/delivery.consumer.ts",
    "import { DeliveryConsumer } from \"../../common/delivery/consumer\";\n\
     import { MailClient } from \"../../infrastructure/mail\";\n\
     export class UserDelivery extends DeliveryConsumer { client = new MailClient(); }\n",
);
const MODULE: (&str, &str) = (
    "src/modules/user/module.ts",
    "import { MailClient } from \"../../infrastructure/mail\";\n\
     export const providers = [MailClient];\n",
);
const SENDER: (&str, &str) = (
    "src/modules/admin/notify.ts",
    "import { MailClient } from \"../../infrastructure/mail/client\";\n\
     export const notify = () => new MailClient().send();\n",
);

fn clean() -> Vec<(&'static str, &'static str)> {
    vec![
        CONSUMER,
        PORT,
        MAIL,
        MAIL_BARREL,
        TRANSPORT,
        PAYMENT,
        PARAMETERS,
        DELIVERY,
        MODULE,
        SENDER,
    ]
}

fn finding(aspect: &str, path: &str, line: Option<u32>, message: &str) -> Row {
    row(aspect, path, line, message)
}

fn outside(path: &str, line: u32, first: &str) -> Row {
    finding(
        "unclassified-egress",
        path,
        Some(line),
        &format!(
            "{path} sends outside the application through {first}, but lies under no egress owner \
             of the nestjs/notification-egress policy; move it under its owner or register its root with a class and an \
             accepted decision"
        ),
    )
}

fn direct(path: &str, line: u32, what: &str) -> Row {
    finding(
        "direct-notification",
        path,
        Some(line),
        &format!(
            "{path} loads {what} of the NOTIFICATION owner src/infrastructure/mail/; a \
             notification leaves only through a class extending DeliveryConsumer, so send it from \
             a delivery consumer"
        ),
    )
}

fn stale(path: &str, line: Option<u32>, message: &str) -> Row {
    finding("stale-owner", path, line, message)
}

const MAIL_UNUSED: &str = "the egress owner src/infrastructure/mail/ has no tracked source file \
                           that uses an egress point; remove it";
const SENDER_UNUSED: &str = "the sender src/modules/admin/notify.ts of the egress owner \
                             src/infrastructure/mail/ is not tracked or loads nothing of that \
                             owner; remove it";
const PAYMENT_UNUSED: &str = "the egress owner src/infrastructure/payment/ has no tracked source \
                              file that uses an egress point; remove it";
const ROOT_UNUSED: &str =
    "the transport root src/infrastructure/transport/ has no tracked source file; remove it";
const SYMBOL_UNUSED: &str = "the transport symbol HttpTransport is declared by no file below \
                             src/infrastructure/transport/; remove it";
const MODULE_UNUSED: &str = "the transport module node:https is loaded by no file below \
                             src/infrastructure/transport/; remove it";
const PLATFORM_UNUSED: &str =
    "the platform client @aws-sdk/client-ssm is loaded by no source file; remove it";

#[test]
fn owned_egress_consumers_and_senders_pass() {
    assert_eq!(check(RULE, OPTIONS, &clean()), []);
}

#[test]
fn egress_outside_an_owner_or_the_transport_list_is_reported() {
    let mut files = clean();
    files.push((
        "src/modules/user/user.service.ts",
        "import type { Agent } from \"node:https\";\n\
         const send = fetch;\n\
         export const a = (agent?: Agent) => globalThis[\"fetch\"];\n\
         export type Fetch = typeof fetch;\n",
    ));
    files.push((
        "src/modules/user/local.ts",
        "export function run(fetch: () => void) { return fetch(); }\n",
    ));
    files.push((
        "src/infrastructure/transport/mail.ts",
        "import { HttpTransport } from \"./http\";\n\
         export const mail = () => import(\"@aws-sdk/client-sesv2\");\n",
    ));
    files.push((
        "src/modules/user/hook.ts",
        "import { HttpTransport } from \"../../infrastructure/transport/http\";\n\
         export const hook = new HttpTransport();\n",
    ));

    assert_eq!(
        check(RULE, OPTIONS, &files),
        [
            finding(
                "unclassified-egress",
                "src/infrastructure/transport/mail.ts",
                Some(2),
                "src/infrastructure/transport/mail.ts sends outside the application through the \
                 module @aws-sdk/client-sesv2, which the transport's module list in the \
                 nestjs/notification-egress policy does not admit; send through the \
                 transport's modules, or widen the list in a reviewed slice that cites D-0003"
            ),
            outside(
                "src/modules/user/hook.ts",
                1,
                "the transport symbol HttpTransport"
            ),
            outside("src/modules/user/user.service.ts", 2, "the global fetch"),
        ]
    );
}

#[test]
fn unclassified_family_members_are_reported_in_every_file() {
    let mut files = clean();
    files.push((
        "src/infrastructure/payment/queue.ts",
        "import type { SQSClient as Type } from \"@aws-sdk/client-sns\";\n\
         // require(\"@aws-sdk/client-sqs\")\n\
         export const sqs = require(\"@aws-sdk/client-sqs\");\n\
         export const ssm = require(\"@aws-sdk/client-ssm/dist\");\n",
    ));
    assert_eq!(
        check(RULE, OPTIONS, &files),
        [finding(
            "unclassified-package",
            "src/infrastructure/payment/queue.ts",
            Some(3),
            "src/infrastructure/payment/queue.ts loads @aws-sdk/client-sqs of the package family \
             @aws-sdk/client-, which the nestjs/notification-egress policy does not classify; \
             list it as an egress module used under its owner, or as a platform client with its \
             decisions"
        )]
    );
}

#[test]
fn notification_owners_are_loaded_only_through_consumers() {
    let mut files = clean();
    files.push((
        "src/modules/user/user.service.ts",
        "import { MailClient } from \"../../infrastructure/mail\";\n\
         import type { MailPort } from \"../../common/port/mail.port\";\n\
         import * as mail from \"../../infrastructure/mail/client\";\n\
         export class UserService { client = new MailClient(); port?: MailPort; m = mail; }\n",
    ));
    let service = "src/modules/user/user.service.ts";

    assert_eq!(
        check(RULE, OPTIONS, &files),
        [
            direct(service, 1, "the module ../../infrastructure/mail"),
            direct(service, 2, "the symbol MailPort"),
            direct(service, 3, "the module ../../infrastructure/mail/client"),
        ]
    );
}

#[test]
fn stale_entries_are_reported_at_the_options_file() {
    let files = vec![CONSUMER, PORT, PARAMETERS, (OPTIONS_FILE, OPTIONS)];
    let found =
        rows(&run_from(RULE, OPTIONS, Some(OPTIONS_FILE), "{}", &files).expect("the rule runs"));
    let file = |line, message| stale(OPTIONS_FILE, Some(line), message);

    assert_eq!(
        found,
        [
            file(19, MAIL_UNUSED),
            file(19, SENDER_UNUSED),
            file(25, PAYMENT_UNUSED),
            file(12, ROOT_UNUSED),
            file(14, SYMBOL_UNUSED),
            file(13, MODULE_UNUSED),
        ]
    );

    let inline = |message| stale("gate.toml", None, message);

    assert_eq!(
        check(RULE, OPTIONS, &[CONSUMER, PORT]),
        [
            inline(MAIL_UNUSED),
            inline(SENDER_UNUSED),
            inline(PAYMENT_UNUSED),
            inline(ROOT_UNUSED),
            inline(SYMBOL_UNUSED),
            inline(MODULE_UNUSED),
            inline(PLATFORM_UNUSED),
        ]
    );
}

#[test]
fn classes_decisions_roots_and_names_are_options() {
    let index = "| `D-0001` | accepted | Mail | `d1.md` |\n\
                 | `D-0002` | proposed | Payment | `d2.md` |\n\
                 | `D-0003` | accepted | Transport | `d3.md` |\n\
                 | `D-0004` | accepted | Parameters | `d4.md` |\n";
    let with_index = OPTIONS.replacen('{', "{ \"decisions\": \"docs/decisions.md\",", 1);
    let mut files = clean();
    files.push(("docs/decisions.md", index));
    let failure = |options: &str, files: &[(&str, &str)], expected: &str| {
        let message = error(RULE, options, files);
        assert!(message.contains(expected), "{message}");
    };

    failure(&with_index, &files, "D-0002 is not an accepted decision");
    failure(
        &with_index.replace("docs/decisions.md", "docs/missing.md"),
        &files,
        "the decision index docs/missing.md is not tracked",
    );
    failure(
        &OPTIONS.replace("\"classes\": [\"PAYMENT\"]", "\"classes\": [\"BILLING\"]"),
        &clean(),
        "names the classes [\"BILLING\"]; give a non-empty set of NOTIFICATION, PAYMENT",
    );
    failure(
        &OPTIONS.replace("src/infrastructure/payment/", "lib/payment/"),
        &clean(),
        "the root \"lib/payment/\" is not a unique src/ directory",
    );
    failure(
        r#"{ "owner-classes": ["A"], "egress": { "modules": [] } }"#,
        &[],
        "missing field `owners`",
    );
    failure(
        &OPTIONS.replace("\"consumer-label\": \"delivery consumer\",", ""),
        &clean(),
        "`notification-class` needs `consumer` and `consumer-label`",
    );
    // Messages name the rule in fixed wording: an option that relabelled them is an unknown key.
    failure(
        &OPTIONS.replace(
            "\"consumer-label\": \"delivery consumer\",",
            "\"consumer-label\": \"delivery consumer\", \"policy-label\": \"egress register\",",
        ),
        &clean(),
        "unknown field `policy-label`",
    );

    // Without a notification class nothing loads a notification owner, so its sender is stale.
    let renamed = OPTIONS.replace(
        "\"notification-class\": \"NOTIFICATION\",",
        "\"global-egress-functions\": [\"send\"],",
    );
    let mut files = clean();
    files.push((
        "src/modules/user/user.service.ts",
        "import { MailClient } from \"../../infrastructure/mail\";\n\
         export const a = () => { fetch(\"x\"); send(); };\n",
    ));
    assert_eq!(
        check(RULE, &renamed, &files),
        [
            outside("src/modules/user/user.service.ts", 2, "the global send"),
            stale("gate.toml", None, SENDER_UNUSED),
        ]
    );
}

#[test]
fn the_walk_skips_the_typescript_skip_folders() {
    let text = "export const send = () => fetch(\"https://example.test\");\n";
    let generated = "src/modules/generated/user/user.service.ts";
    let vendored = "src/node_modules/pkg/index.ts";
    let mut files = clean();
    files.extend([(generated, text), (vendored, text)]);
    let paths = |languages: &str| support::paths_with(RULE, OPTIONS, "{}", languages, &files);

    assert_eq!(paths("{}"), [generated]);
    assert_eq!(paths(support::SKIP_GENERATED), [vendored]);
}
