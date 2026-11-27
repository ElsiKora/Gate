//! `nestjs/typeorm-lock-owner` against synthetic trees: a base tree whose one owner builds the row
//! lock options, plus one case of files per test.

mod host;

use host::{Request, Row, check, error, row, run_with};

const RULE: &str = "typeorm-lock-owner";
const OPTIONS: &str = r#"{ "owner": "src/lock/row-lock.ts" }"#;
const OWNER: &str = "src/lock/row-lock.ts";

const ROW_LOCK: &str = r#"import type { FindOneOptions, ObjectLiteral, Repository } from "typeorm";

import type { RowLockStrength } from "./strength";
import type { RowLockWaitPolicy } from "./wait-policy";

export type RowLockTarget = ReadonlyArray<Repository<ObjectLiteral>> | Repository<ObjectLiteral>;

export function lockRows(
  repositories: RowLockTarget,
  strength: RowLockStrength,
  waitPolicy?: RowLockWaitPolicy,
): NonNullable<FindOneOptions<ObjectLiteral>["lock"]> {
  const list: ReadonlyArray<Repository<ObjectLiteral>> = Array.isArray(repositories) ? repositories : [repositories];

  return {
    mode: strength,
    onLocked: waitPolicy,
    tables: list.map((repository: Repository<ObjectLiteral>): string => repository.metadata.tableNameWithoutPrefix),
  };
}
"#;

const STRENGTH: &str = r#"export enum RowLockStrength {
  FOR_KEY_SHARE = "for_key_share",
  FOR_NO_KEY_UPDATE = "for_no_key_update",
  FOR_SHARE = "pessimistic_read",
  FOR_UPDATE = "pessimistic_write",
}
"#;

const WAIT_POLICY: &str = r#"export enum RowLockWaitPolicy {
  SKIP_LOCKED = "skip_locked",
}
"#;

const BARREL: &str = r#"export { lockRows } from "./row-lock";
export { RowLockStrength } from "./strength";
export { RowLockWaitPolicy } from "./wait-policy";
"#;

const BOOK_ENTITY: &str = r#"import { Entity, PrimaryColumn } from "typeorm";

@Entity("book")
export class BookEntity {
  @PrimaryColumn()
  id!: string;
}
"#;

const SHELF_ENTITY: &str = r#"import { Entity, PrimaryColumn } from "typeorm";

@Entity("shelf")
export class ShelfEntity {
  @PrimaryColumn()
  id!: string;
}
"#;

const SHELF_SERVICE: &str = r#"import { ApiServiceBase } from "@elsikora/nestjs-crud-automator";

import { ShelfEntity } from "./entity";

export class ShelfService extends ApiServiceBase<ShelfEntity> {}
"#;

/// Every repository, entity-manager and builder form, with the three named false positives.
const PASS_READS: &str = r#"import type { EntityManager, Repository } from "typeorm";

import { RowLockStrength, RowLockWaitPolicy, lockRows } from "../../lock";
import type { ShelfEntity } from "../../shelf/entity";
import { BookEntity } from "../entity";

enum TransferMode {
  STRICT = "strict",
}

export async function readBooks(
  manager: EntityManager,
  repository: Repository<BookEntity>,
  shelfRepository: Repository<ShelfEntity>,
  mutex: { setLock(value: boolean): void },
  isWrite: boolean,
  shouldLock: boolean,
): Promise<void> {
  await repository.findOne({
    lock: lockRows(repository, RowLockStrength.FOR_UPDATE),
    where: { id: "a" },
  });

  await manager.getRepository(BookEntity).find({
    lock: lockRows(
      manager.getRepository(BookEntity),
      isWrite ? RowLockStrength.FOR_UPDATE : RowLockStrength.FOR_SHARE,
    ),
  });

  await manager.findOne(BookEntity, {
    lock: lockRows(manager.getRepository(BookEntity), RowLockStrength.FOR_KEY_SHARE),
    where: { id: "a" },
  });

  await repository.find({
    ...(shouldLock ? { lock: lockRows(repository, RowLockStrength.FOR_UPDATE) } : {}),
  });

  await repository.findOne(
    shouldLock ? { lock: lockRows(repository, RowLockStrength.FOR_UPDATE), where: { id: "a" } } : { where: { id: "a" } },
  );

  const builder = repository.createQueryBuilder("book").innerJoin("book.shelf", "shelf");

  await builder
    .setFindOptions({
      loadEagerRelations: false,
      lock: lockRows(repository, RowLockStrength.FOR_NO_KEY_UPDATE, RowLockWaitPolicy.SKIP_LOCKED),
    })
    .limit(1)
    .getOne();

  await repository
    .createQueryBuilder("book")
    .innerJoin("book.shelf", "shelf")
    .setFindOptions({
      loadEagerRelations: false,
      lock: lockRows([shelfRepository, repository], RowLockStrength.FOR_UPDATE),
    })
    .getOne();

  mutex.setLock(true);

  const transfer = { mode: TransferMode.STRICT, retries: { mode: "linear" } };
  void transfer;
}
"#;

/// The service's own read, another service's read, an unrelated `mode`, and `this.get` outside a
/// service.
const PASS_SERVICE: &str = r#"import { ApiServiceBase } from "@elsikora/nestjs-crud-automator";

import { RowLockStrength, RowLockWaitPolicy, lockRows } from "../lock";
import { ShelfEntity } from "../shelf/entity";
import type { ShelfService } from "../shelf/service";
import { BookEntity } from "./entity";

enum Transaction {
  MANDATORY = "mandatory",
}

function Step(options: object): MethodDecorator {
  void options;
  return () => undefined;
}

export class BookService extends ApiServiceBase<BookEntity> {
  constructor(private readonly shelfService: ShelfService) {
    super();
  }

  @Step({ entity: BookEntity, transaction: { mode: Transaction.MANDATORY } })
  async expirePending(): Promise<void> {
    await this.getMany({
      lock: lockRows(
        this.getApiFunctionStepContext<BookEntity>().getRepository(BookEntity),
        RowLockStrength.FOR_UPDATE,
        RowLockWaitPolicy.SKIP_LOCKED,
      ),
      order: { id: "ASC" },
    });

    await this.shelfService.getMany({
      lock: lockRows(this.getApiFunctionStepContext<ShelfEntity>().getRepository(ShelfEntity), RowLockStrength.FOR_UPDATE),
      order: { id: "ASC" },
    });
  }
}

export class BookCache {
  get(options: object): object {
    return options;
  }

  read(): object {
    return this.get({ lock: { mode: "pessimistic_write" } });
  }
}
"#;

/// The base tree plus `case`.
fn tree<'a>(case: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut files = vec![
        (OWNER, ROW_LOCK),
        ("src/lock/strength.ts", STRENGTH),
        ("src/lock/wait-policy.ts", WAIT_POLICY),
        ("src/lock/index.ts", BARREL),
        ("src/book/entity.ts", BOOK_ENTITY),
        ("src/shelf/entity.ts", SHELF_ENTITY),
        ("src/shelf/service.ts", SHELF_SERVICE),
    ];
    files.extend_from_slice(case);
    files
}

fn findings(case: &[(&str, &str)]) -> Vec<Row> {
    check(RULE, OPTIONS, &tree(case))
}

/// The one-based line of the first line of `text` that holds `needle`.
fn line(text: &str, needle: &str) -> u32 {
    let index = text
        .lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} is in the fixture"));
    u32::try_from(index + 1).expect("a small fixture")
}

/// The `n`-th (one-based) line of `text` that holds `needle`.
fn nth_line(text: &str, needle: &str, n: usize) -> u32 {
    let index = text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(needle))
        .nth(n - 1)
        .map_or_else(
            || panic!("{needle:?} is in the fixture {n} times"),
            |(index, _)| index,
        );
    u32::try_from(index + 1).expect("a small fixture")
}

fn lock_object(path: &str, line: u32) -> Row {
    row(
        "lock-object",
        path,
        Some(line),
        &format!(
            "{path}:{line} builds a row lock object outside {OWNER}; take the lock from lockRows."
        ),
    )
}

fn lock_value(path: &str, line: u32) -> Row {
    row(
        "lock-value",
        path,
        Some(line),
        &format!("{path}:{line} sets lock to a value that is not a call of lockRows."),
    )
}

fn stray(path: &str, line: u32) -> Row {
    row(
        "lock-value",
        path,
        Some(line),
        &format!("{path}:{line} calls lockRows outside the lock property of a read."),
    )
}

#[test]
fn the_base_tree_and_every_recognised_form_pass() {
    assert_eq!(findings(&[]), []);
    assert_eq!(
        findings(&[
            ("src/book/function/read.ts", PASS_READS),
            ("src/book/service.ts", PASS_SERVICE),
        ]),
        []
    );
}

#[test]
fn a_hand_written_lock_object_is_reported_at_its_lock() {
    let plain = r#"import type { Repository } from "typeorm";

import type { BookEntity } from "./entity";

export async function readBook(repository: Repository<BookEntity>): Promise<void> {
  await repository.findOne({
    lock: { mode: "pessimistic_write" },
    where: { id: "a" },
  });
}
"#;
    let conditional = r#"import type { Repository } from "typeorm";

import type { BookEntity } from "./entity";

export async function readBook(repository: Repository<BookEntity>, isWrite: boolean): Promise<void> {
  await repository.findOne({
    lock: { mode: isWrite ? "pessimistic_write" : "pessimistic_read" },
    where: { id: "a" },
  });
}
"#;
    let spread = r#"import type { Repository } from "typeorm";

import type { BookEntity } from "./entity";

export async function readBooks(repository: Repository<BookEntity>, shouldLock: boolean): Promise<void> {
  await repository.find({
    ...(shouldLock ? { lock: { mode: "pessimistic_write" as const } } : {}),
  });
}
"#;

    assert_eq!(
        findings(&[
            ("src/book/conditional.ts", conditional),
            ("src/book/plain.ts", plain),
            ("src/book/spread.ts", spread),
        ]),
        [
            lock_object("src/book/conditional.ts", line(conditional, "lock:")),
            lock_object("src/book/plain.ts", line(plain, "lock:")),
            lock_object("src/book/spread.ts", line(spread, "lock:")),
        ]
    );
}

#[test]
fn a_lock_object_built_by_a_helper_is_reported_at_its_mode() {
    let helper = r#"import type { Repository } from "typeorm";

import { RowLockStrength } from "../lock";
import type { BookEntity } from "./entity";

function bookLock() {
  return { mode: RowLockStrength.FOR_UPDATE, tables: ["book"] };
}

export async function readBook(repository: Repository<BookEntity>): Promise<void> {
  await repository.findOne({ lock: bookLock(), where: { id: "a" } });
}
"#;
    let typed = r#"import type { Repository } from "typeorm";

import { RowLockStrength } from "../lock";
import type { BookEntity } from "./entity";

function lockFor(strength: RowLockStrength) {
  return { mode: strength, tables: ["book"] };
}

export async function readBook(repository: Repository<BookEntity>): Promise<void> {
  await repository.findOne({ lock: lockFor(RowLockStrength.FOR_UPDATE), where: { id: "a" } });
}
"#;

    assert_eq!(
        findings(&[("src/book/helper.ts", helper), ("src/book/typed.ts", typed)]),
        [
            lock_object("src/book/helper.ts", line(helper, "mode:")),
            lock_value("src/book/helper.ts", line(helper, "lock:")),
            lock_object("src/book/typed.ts", line(typed, "mode:")),
            lock_value("src/book/typed.ts", line(typed, "lock:")),
        ]
    );
}

#[test]
fn a_local_namesake_or_a_shadowing_binding_is_not_the_owner() {
    let namesake = r#"import type { Repository } from "typeorm";

import { RowLockStrength } from "../lock";
import type { BookEntity } from "./entity";

function lockRows(repository: Repository<BookEntity>, strength: RowLockStrength) {
  return { mode: strength, tables: [repository.metadata.tableNameWithoutPrefix] };
}

export async function readBook(repository: Repository<BookEntity>): Promise<void> {
  await repository.findOne({ lock: lockRows(repository, RowLockStrength.FOR_UPDATE), where: { id: "a" } });
}
"#;
    let shadow = r#"import type { Repository } from "typeorm";

import { RowLockStrength, lockRows } from "../lock";
import type { BookEntity } from "./entity";

export async function readBook(repository: Repository<BookEntity>, lockRows: (r: unknown, s: RowLockStrength) => object): Promise<void> {
  await repository.findOne({ lock: lockRows(repository, RowLockStrength.FOR_UPDATE) });
}
"#;

    assert_eq!(
        findings(&[
            ("src/book/namesake.ts", namesake),
            ("src/book/shadow.ts", shadow)
        ]),
        [
            lock_object("src/book/namesake.ts", line(namesake, "mode:")),
            lock_value("src/book/namesake.ts", line(namesake, "lock:")),
            lock_value("src/book/shadow.ts", line(shadow, "lock:")),
        ]
    );
}

#[test]
fn an_owner_call_held_in_a_variable_is_reported_twice() {
    let held = r#"import type { Repository } from "typeorm";

import { RowLockStrength, lockRows } from "../lock";
import type { BookEntity } from "./entity";

export async function readBook(repository: Repository<BookEntity>): Promise<void> {
  const lock = lockRows(repository, RowLockStrength.FOR_UPDATE);

  await repository.findOne({ lock, where: { id: "a" } });
}
"#;
    let path = "src/book/held.ts";

    assert_eq!(
        findings(&[(path, held)]),
        [
            stray(path, line(held, "const lock")),
            lock_value(path, line(held, "{ lock,")),
        ]
    );
}

#[test]
fn modes_that_lock_no_row_are_skipped_and_other_modes_are_lock_values() {
    let modes = r#"import type { Repository } from "typeorm";

import type { BookEntity } from "./entity";

export async function readBook(repository: Repository<BookEntity>, mode: string): Promise<void> {
  await repository.findOne({ lock: { mode: "optimistic", version: 2 }, where: { id: "a" } });
  await repository.findOne({ lock: { mode: "dirty_read" } });
  await repository.findOne({ lock: { mode: "pessimistic_wirte" } });
  await repository.findOne({ lock: { mode } });
}
"#;
    let path = "src/book/modes.ts";

    assert_eq!(
        findings(&[(path, modes)]),
        [
            lock_value(path, line(modes, "pessimistic_wirte")),
            lock_value(path, line(modes, "{ mode }")),
        ]
    );
}

#[test]
fn a_lock_outside_a_read_is_not_read_unless_it_holds_an_owner_call() {
    let options = r#"import type { Repository } from "typeorm";

import { RowLockStrength, lockRows } from "../lock";
import type { BookEntity } from "./entity";

export function addBookLock(repository: Repository<BookEntity>, properties: object): object {
  return { ...properties, lock: lockRows(repository, RowLockStrength.FOR_UPDATE) };
}

export const flags = { lock: true, where: { lock: "column" } };

export async function readBy(repository: Repository<BookEntity>): Promise<void> {
  await repository.findOneBy({ lock: true });
}
"#;
    let path = "src/book/options.ts";
    let at = line(options, "return {");

    assert_eq!(
        findings(&[(path, options)]),
        [row(
            "receiver",
            path,
            Some(at),
            &format!(
                "{path}:{at} passes lock in options that are not an argument of a read, so the repository it locks cannot be checked."
            )
        )]
    );
}

#[test]
fn a_lock_on_another_repository_is_reported_for_each_read_form() {
    let reader = r#"import type { EntityManager, Repository } from "typeorm";

import { RowLockStrength, lockRows } from "../lock";
import { ShelfEntity } from "../shelf/entity";
import { BookEntity } from "./entity";

export async function readBook(
  manager: EntityManager,
  repository: Repository<BookEntity>,
  shelfRepository: Repository<ShelfEntity>,
): Promise<void> {
  await repository.findOne({ lock: lockRows(shelfRepository, RowLockStrength.FOR_UPDATE), where: { id: "a" } });

  await manager.findOne(BookEntity, { lock: lockRows(manager.getRepository(ShelfEntity), RowLockStrength.FOR_UPDATE) });

  await repository
    .createQueryBuilder("book")
    .setFindOptions({ loadEagerRelations: false, lock: lockRows(shelfRepository, RowLockStrength.FOR_UPDATE) })
    .getOne();

  await manager.findOne("BookEntity", { lock: lockRows(repository, RowLockStrength.FOR_UPDATE) });
}
"#;
    let path = "src/book/reader.ts";
    let receiver = |line: u32, argument: &str, expected: &str| {
        row(
            "receiver",
            path,
            Some(line),
            &format!("{path}:{line} locks through {argument}, but the read runs on {expected}."),
        )
    };
    let unnamed = line(reader, "\"BookEntity\"");

    assert_eq!(
        findings(&[(path, reader)]),
        [
            receiver(
                nth_line(reader, "lockRows(shelfRepository", 1),
                "shelfRepository",
                "repository"
            ),
            receiver(
                line(reader, "manager.findOne(BookEntity"),
                "manager.getRepository(ShelfEntity)",
                "manager.getRepository(BookEntity)"
            ),
            receiver(
                nth_line(reader, "lockRows(shelfRepository", 2),
                "shelfRepository",
                "repository"
            ),
            row(
                "receiver",
                path,
                Some(unnamed),
                &format!(
                    "{path}:{unnamed} locks a read whose repository cannot be named; call the read on a repository, as manager.<read>(<Entity>, …) or on a chain from createQueryBuilder."
                )
            ),
        ]
    );
}

#[test]
fn the_same_repository_is_compared_as_a_tree_through_one_const() {
    let reader = r#"import type { Repository } from "typeorm";

import { RowLockStrength, lockRows } from "../lock";
import type { BookEntity } from "./entity";

export async function readBook(context: { getRepository(entity: unknown): Repository<BookEntity> }, entity: unknown): Promise<void> {
  const books = context.getRepository(entity);

  await context.getRepository(entity).findOne({ lock: lockRows(books, RowLockStrength.FOR_UPDATE) });
  await books.findOne({ lock: lockRows((context /* the context */).getRepository(entity), RowLockStrength.FOR_UPDATE) });
  await books.findOne({ lock: lockRows(context.getRepository(books), RowLockStrength.FOR_UPDATE) });
}
"#;
    let path = "src/book/compare.ts";
    let at = line(reader, "context.getRepository(books)");

    assert_eq!(
        findings(&[(path, reader)]),
        [row(
            "receiver",
            path,
            Some(at),
            &format!(
                "{path}:{at} locks through context.getRepository(books), but the read runs on books."
            )
        )]
    );
}

#[test]
fn a_service_read_needs_the_step_context_repository_of_its_entity() {
    let service = r#"import { ApiServiceBase } from "@elsikora/nestjs-crud-automator";

import { RowLockStrength, lockRows } from "../lock";
import { ShelfEntity } from "../shelf/entity";
import type { ShelfService } from "../shelf/service";
import { BookEntity } from "./entity";

class Plain {
  getMany(options: object): object {
    return options;
  }
}

export class BookService extends ApiServiceBase<BookEntity> {
  constructor(
    private readonly shelfService: ShelfService,
    private readonly plain: Plain,
  ) {
    super();
  }

  async expirePending(): Promise<void> {
    await this.getMany({ lock: lockRows(this.repository, RowLockStrength.FOR_UPDATE) });

    await this.getMany({
      lock: lockRows(this.getApiFunctionStepContext<ShelfEntity>().getRepository(ShelfEntity), RowLockStrength.FOR_UPDATE),
    });

    await this.getMany({
      lock: lockRows(this.getApiFunctionStepContext<BookEntity>().getRepository(ShelfEntity), RowLockStrength.FOR_UPDATE),
    });

    await this.shelfService.getMany({
      lock: lockRows(this.getApiFunctionStepContext<BookEntity>().getRepository(BookEntity), RowLockStrength.FOR_UPDATE),
    });

    await this.plain.getMany({
      lock: lockRows(this.getApiFunctionStepContext<ShelfEntity>().getRepository(ShelfEntity), RowLockStrength.FOR_UPDATE),
    });

    await this.plain.getMany({
      lock: lockRows(this.getApiFunctionStepContext<ShelfEntity>().getRepository(BookEntity), RowLockStrength.FOR_UPDATE),
    });
  }
}
"#;
    let path = "src/book/service.ts";
    let own = |line: u32, argument: &str, entity: &str| {
        row(
            "receiver",
            path,
            Some(line),
            &format!(
                "{path}:{line} locks a read of a service through {argument}; pass this.getApiFunctionStepContext<{entity}>().getRepository({entity}) with the service's entity {entity}."
            ),
        )
    };
    let shape = nth_line(service, "getRepository(BookEntity), RowLockStrength", 2);

    assert_eq!(
        findings(&[(path, service)]),
        [
            own(
                line(service, "this.repository"),
                "this.repository",
                "BookEntity"
            ),
            own(
                nth_line(service, "<ShelfEntity>().getRepository(ShelfEntity)", 1),
                "this.getApiFunctionStepContext<ShelfEntity>().getRepository(ShelfEntity)",
                "BookEntity"
            ),
            own(
                line(service, "<BookEntity>().getRepository(ShelfEntity)"),
                "this.getApiFunctionStepContext<BookEntity>().getRepository(ShelfEntity)",
                "BookEntity"
            ),
            own(
                nth_line(service, "<BookEntity>().getRepository(BookEntity)", 1),
                "this.getApiFunctionStepContext<BookEntity>().getRepository(BookEntity)",
                "ShelfEntity"
            ),
            row(
                "receiver",
                path,
                Some(shape),
                &format!(
                    "{path}:{shape} locks a read of a service through this.getApiFunctionStepContext<ShelfEntity>().getRepository(BookEntity); pass this.getApiFunctionStepContext<Entity>().getRepository(Entity) with one entity as both the type argument and the argument."
                )
            ),
        ]
    );
}

#[test]
fn a_list_of_repositories_is_only_for_a_builder_and_holds_distinct_rows() {
    let reader = r#"import type { Repository } from "typeorm";

import { RowLockStrength, lockRows } from "../lock";
import type { ShelfEntity } from "../shelf/entity";
import type { BookEntity } from "./entity";

export async function readBook(repository: Repository<BookEntity>, shelfRepository: Repository<ShelfEntity>): Promise<void> {
  await repository.findOne({
    lock: lockRows([repository, shelfRepository], RowLockStrength.FOR_UPDATE),
    relations: { shelf: true },
  });

  await repository
    .createQueryBuilder("book")
    .setFindOptions({ loadEagerRelations: false, lock: lockRows([repository, (repository)], RowLockStrength.FOR_UPDATE) })
    .getOne();

  await repository
    .createQueryBuilder("book")
    .setFindOptions({ loadEagerRelations: false, lock: lockRows([], RowLockStrength.FOR_UPDATE) })
    .getOne();
}
"#;
    let path = "src/book/reader.ts";
    let outside = line(reader, "lockRows([repository, shelfRepository]");
    let repeated = line(reader, "(repository)]");
    let empty = line(reader, "lockRows([]");

    assert_eq!(
        findings(&[(path, reader)]),
        [
            row(
                "row-list",
                path,
                Some(outside),
                &format!(
                    "{path}:{outside} passes a list of repositories outside a query builder's setFindOptions; only a builder locks joined rows."
                )
            ),
            row(
                "row-list",
                path,
                Some(repeated),
                &format!(
                    "{path}:{repeated} names (repository) more than once in its list of locked rows."
                )
            ),
            row(
                "row-list",
                path,
                Some(empty),
                &format!("{path}:{empty} passes an empty list of locked rows.")
            ),
        ]
    );
}

#[test]
fn a_builder_locks_only_through_set_find_options_without_eager_relations() {
    let reader = r#"import type { Repository, SelectQueryBuilder } from "typeorm";

import { RowLockStrength, lockRows } from "../lock";
import type { BookEntity } from "./entity";

export async function readBook(repository: Repository<BookEntity>, query: SelectQueryBuilder<BookEntity>): Promise<void> {
  await repository
    .createQueryBuilder("book")
    .setLock("pessimistic_write", undefined, ["book"])
    .getOne();

  await repository
    .createQueryBuilder("book")
    .setFindOptions({ loadEagerRelations: false, lock: lockRows(repository, RowLockStrength.FOR_UPDATE) })
    .setOnLocked("skip_locked")
    .limit(1)
    .getOne();

  await repository
    .createQueryBuilder("book")
    .setFindOptions({ lock: lockRows(repository, RowLockStrength.FOR_UPDATE) })
    .getOne();

  await query.innerJoin("book.shelf", "shelf").setLock("pessimistic_read").getOne();
}
"#;
    let path = "src/book/reader.ts";
    let set_lock = line(reader, ".setLock(\"pessimistic_write\"");
    let on_locked = line(reader, ".setOnLocked");
    let eager = line(reader, ".setFindOptions({ lock:");
    let typed = line(reader, "query.innerJoin");

    assert_eq!(
        findings(&[(path, reader)]),
        [
            row(
                "builder",
                path,
                Some(set_lock),
                &format!(
                    "{path}:{set_lock} calls setLock; take the lock through setFindOptions with lockRows."
                )
            ),
            row(
                "builder",
                path,
                Some(on_locked),
                &format!(
                    "{path}:{on_locked} calls setOnLocked; take the lock through setFindOptions with lockRows."
                )
            ),
            row(
                "builder",
                path,
                Some(eager),
                &format!(
                    "{path}:{eager} passes lock to setFindOptions without loadEagerRelations: false."
                )
            ),
            row(
                "builder",
                path,
                Some(typed),
                &format!(
                    "{path}:{typed} calls setLock; take the lock through setFindOptions with lockRows."
                )
            ),
        ]
    );
}

#[test]
fn an_owner_imported_through_a_tsconfig_alias_is_the_owner() {
    let tsconfig =
        r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@lock": ["src/lock/index.ts"] } } }"#;
    let reader = r#"import type { Repository } from "typeorm";

import * as locks from "@lock";
import { RowLockStrength, lockRows as rows } from "@lock";
import type { BookEntity } from "./entity";

export async function readBook(repository: Repository<BookEntity>): Promise<void> {
  await repository.findOne({ lock: rows(repository, RowLockStrength.FOR_UPDATE) });
  await repository.findOne({ lock: locks.lockRows(repository, locks.RowLockStrength.FOR_SHARE) });
}
"#;

    assert_eq!(
        findings(&[("tsconfig.json", tsconfig), ("src/book/reader.ts", reader)]),
        []
    );
}

#[test]
fn a_file_that_does_not_parse_is_unreadable() {
    let path = "src/book/broken.ts";

    assert_eq!(
        findings(&[(path, "export function broken( {\n")]),
        [row(
            "unreadable",
            path,
            None,
            &format!("{path} does not parse, so its row locks are not checked.")
        )]
    );
}

#[test]
fn an_owner_that_does_not_export_exactly_one_function_stops_the_run() {
    let pass = [
        ("src/book/function/read.ts", PASS_READS),
        ("src/book/service.ts", PASS_SERVICE),
    ];
    let missing = r#"{ "owner": "src/lock/missing.ts" }"#;
    let two = "export function lockRows(): object { return {}; }\nexport const other = 1;\n";
    let message = |owner: &str| {
        format!(
            "{owner} is not a tracked TypeScript file that exports exactly one function, so no row lock can be checked against it."
        )
    };

    assert_eq!(
        check(RULE, missing, &tree(&pass)),
        [row(
            "owner",
            "gate.toml",
            None,
            &message("src/lock/missing.ts")
        )]
    );

    let mut files = tree(&pass);
    files[0] = (OWNER, two);
    let findings = run_with(
        RULE,
        &Request {
            options: OPTIONS,
            options_file: Some("gate/options/lock.json"),
            ..Request::default()
        },
        &files,
    )
    .expect("the rule runs");

    assert_eq!(
        host::rows(&findings),
        [row(
            "owner",
            "gate/options/lock.json",
            None,
            &message(OWNER)
        )]
    );
}

#[test]
fn a_default_or_arrow_export_beside_type_exports_is_the_owner() {
    let arrow = "import type { Repository } from \"typeorm\";\n\
                 export type Strength = string;\n\
                 export interface Lock { mode: string }\n\
                 export const lockRows = (repository: Repository<object>, strength: Strength): Lock => ({ mode: strength, tables: [String(repository)] });\n";
    let default = "export default function lockRows(repository: object, strength: string): object {\n\
                   \x20 return { mode: strength, tables: [String(repository)] };\n\
                   }\n";
    let caller = "import type { Repository } from \"typeorm\";\n\
                  import lock from \"../lock/row-lock\";\n\
                  export async function readBook(repository: Repository<object>): Promise<void> {\n\
                  \x20 await repository.findOne({ lock: lock(repository, \"pessimistic_write\") });\n\
                  }\n";

    let mut files = tree(&[("src/book/reader.ts", caller)]);
    files[0] = (OWNER, arrow);
    files[3] = (
        "src/lock/index.ts",
        "export { RowLockStrength } from \"./strength\";\n",
    );
    let arrow_findings = check(RULE, OPTIONS, &files);
    let at = 4;

    assert_eq!(
        arrow_findings,
        [lock_value("src/book/reader.ts", at)],
        "the default import names no export of an owner whose function is named"
    );

    files[0] = (OWNER, default);
    assert_eq!(check(RULE, OPTIONS, &files), []);
}

#[test]
fn findings_are_keyed_by_their_function_and_ordinal() {
    let reader = r#"import type { Repository } from "typeorm";

import type { BookEntity } from "./entity";

export class BookReader {
  async read(repository: Repository<BookEntity>): Promise<void> {
    await repository.findOne({ lock: { mode: "pessimistic_write" } });
    await repository.findOne({ lock: { mode: "pessimistic_read" } });
  }
}

export const readAll = async (repository: Repository<BookEntity>): Promise<void> => {
  await repository.find({ lock: { mode: "pessimistic_write" } });
};
"#;
    let findings = run_with(
        RULE,
        &Request {
            options: OPTIONS,
            ..Request::default()
        },
        &tree(&[("src/book/reader.ts", reader)]),
    )
    .expect("the rule runs");
    let keys: Vec<Option<String>> = findings.iter().map(|finding| finding.key.clone()).collect();

    assert_eq!(
        keys,
        [
            Some("BookReader.read#1".to_owned()),
            Some("BookReader.read#2".to_owned()),
            Some("readAll#1".to_owned()),
        ]
    );
    assert!(findings.iter().all(|finding| finding.target.is_none()));
}

#[test]
fn options_are_checked() {
    assert!(error(RULE, "{}", &tree(&[])).contains("owner"));
    assert!(
        error(RULE, r#"{ "owner": "./src/lock/row-lock.ts" }"#, &tree(&[]))
            .contains("src/lock/row-lock.ts")
    );
    assert!(
        error(
            RULE,
            r#"{ "owner": "src/lock/row-lock.ts", "guarded": true }"#,
            &tree(&[])
        )
        .contains("guarded")
    );
}
