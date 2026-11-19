//! `nestjs/typeorm-relation` against synthetic entity trees.

mod host;

use host::{Request, Row, SKIP_DIST, error, paths_with, row, rows, run, run_with, targets};

const RULE: &str = "typeorm-relation";
const UPPER: &str = r#"{ "foreign-key-case": "upper", "foreign-key-label": "LIBRARY" }"#;
const UPPER_PATTERN: &str = "/^FK_[a-z][a-z0-9]*(?:_[a-z0-9]+)+$/u";

/// The findings of the rule with `options` over `files`, as rows.
fn check(options: &str, files: &[(&str, &str)]) -> Vec<Row> {
    rows(&run(RULE, options, "{}", files).expect("the rule runs"))
}

const LANGUAGE_ENUM: &str = "src/modules/domain/language/enum/code.enum.ts";
const LANGUAGE_ENUM_TEXT: &str = "export enum ELanguageCode {\n EN = \"EN\",\n FR = \"FR\",\n}\n";
const LANGUAGE_ENUMS: &str = "src/modules/domain/language/enum/index.ts";
const LANGUAGE_ENUMS_TEXT: &str = "export { ELanguageCode } from \"./code.enum\";\n";
const LANGUAGE: &str = "src/modules/domain/language/entity/entity.ts";
const LANGUAGE_TEXT: &str = r#"import { Column, Entity, PrimaryColumn } from "typeorm";

import { ELanguageCode } from "../enum";

@Entity("language")
export class LanguageEntity {
 @PrimaryColumn({ type: "enum", enum: ELanguageCode })
 code!: ELanguageCode;
}
"#;
const LANGUAGES: &str = "src/modules/domain/language/entity/index.ts";
const LANGUAGES_TEXT: &str = "export { LanguageEntity } from \"./entity\";\n";
const BOOK: &str = "src/modules/domain/book/entity/entity.ts";
const BOOK_TEXT: &str = r#"import type { Relation } from "typeorm";

import * as orm from "typeorm";

import { LanguageEntity } from "@modules/domain/language/entity";
import { ELanguageCode } from "../../language/enum";

@orm.Entity("book")
export class BookEntity {
 @orm.PrimaryGeneratedColumn("uuid")
 id!: string;

 @orm.Column({ type: "enum", enum: ELanguageCode })
 languageCode!: ELanguageCode;

 @orm.Column({ nullable: true, type: "uuid" })
 parentBookId!: string | null;

 @orm.Column({ type: "uuid" })
 sourceBookId!: string;

 @orm.JoinColumn({ foreignKeyConstraintName: "FK_book_language", name: "languageCode", referencedColumnName: "code" })
 @orm.ManyToOne(() => LanguageEntity, { eager: false, nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 language!: Relation<LanguageEntity>;

 @orm.JoinColumn({ foreignKeyConstraintName: "FK_book_parent", name: "parentBookId", referencedColumnName: "id" })
 @orm.ManyToOne(() => BookEntity, { nullable: true, onDelete: "SET NULL", onUpdate: "NO ACTION" })
 parentBook!: Relation<BookEntity> | null;

 @orm.JoinColumn({ foreignKeyConstraintName: "FK_book_source", name: "sourceBookId", referencedColumnName: "id" })
 @orm.ManyToOne(() => BookEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 sourceBook!: Relation<BookEntity>;
}
"#;
const SHELF: &str = "src/modules/domain/shelf/entity/entity.ts";
const SHELF_TEXT: &str = r#"import type { Relation } from "typeorm";

import { Column, Entity, JoinColumn, ManyToOne, PrimaryColumn } from "typeorm";

@Entity("shelf")
export class ShelfEntity {
 @PrimaryColumn({ nullable: false, type: "uuid" })
 id!: string;

 @Column({ nullable: false, type: "uuid" })
 rootShelfId!: string;

 @JoinColumn({ foreignKeyConstraintName: "FK_shelf_root", name: "rootShelfId", referencedColumnName: "id" })
 @ManyToOne(() => ShelfEntity, { nullable: true, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 rootShelf!: Relation<ShelfEntity>;
}
"#;
const SHELF_DEPARTING: &str = r#"import { Column, Entity, JoinColumn, ManyToOne, PrimaryColumn, type Relation } from "typeorm";

@Entity("shelf")
export class ShelfEntity {
 @PrimaryColumn({ nullable: false, type: "uuid" })
 id!: string;

 @Column({ nullable: true, type: "uuid" })
 rootShelfId!: string | null;

 @JoinColumn({ foreignKeyConstraintName: "FK_shelf_root", name: "rootShelfId", referencedColumnName: "id" })
 @ManyToOne(() => ShelfEntity, { nullable: true, onDelete: "RESTRICT", onUpdate: "CASCADE" })
 rootShelf!: Relation<ShelfEntity>;
}
"#;

/// Upper-case foreign keys and the nullable root shelf relation.
fn with_nullable_shelf(reason: &str) -> String {
    format!(
        r#"{{ "foreign-key-case": "upper", "foreign-key-label": "LIBRARY", "nullable-relations": [{{
            "path": "{SHELF}", "class": "ShelfEntity", "member": "rootShelf",
            "target-path": "{SHELF}", "target-class": "ShelfEntity",
            "local-name": "rootShelfId", "referenced-name": "id", "scalar": "string",
            "on-delete": "RESTRICT", "on-update": "NO ACTION"{reason} }}] }}"#
    )
}

#[test]
fn complete_relations_pass_and_a_nullable_relation_is_reported_for_its_exception() {
    let findings = run(
        RULE,
        &with_nullable_shelf(r#", "reason": "insertion cycle""#),
        "{}",
        &[
            (LANGUAGE_ENUM, LANGUAGE_ENUM_TEXT),
            (LANGUAGE_ENUMS, LANGUAGE_ENUMS_TEXT),
            (LANGUAGE, LANGUAGE_TEXT),
            (LANGUAGES, LANGUAGES_TEXT),
            (BOOK, BOOK_TEXT),
            (SHELF, SHELF_TEXT),
        ],
    )
    .expect("the rule runs");

    assert_eq!(
        rows(&findings),
        [row(
            "relation-nullability",
            SHELF,
            Some(13),
            &format!(
                "{SHELF}:13 ShelfEntity.rootShelf @ManyToOne option nullable=true must match relation property nullability=false"
            )
        )]
    );
    assert_eq!(
        targets(&findings),
        [Some(format!("{SHELF}:ShelfEntity.rootShelf"))]
    );
}

#[test]
fn a_nullable_relation_that_departs_from_its_contract_is_reported() {
    let label = format!("{SHELF}:11 ShelfEntity.rootShelf");
    let findings = run(
        RULE,
        &with_nullable_shelf(r#", "reason": "insertion cycle""#),
        "{}",
        &[(SHELF, SHELF_DEPARTING)],
    )
    .expect("the rule runs");

    assert_eq!(
        rows(&findings),
        [
            row(
                "relation-nullability",
                SHELF,
                Some(11),
                &format!(
                    "{label} @ManyToOne option nullable=true must match relation property nullability=false"
                )
            ),
            row(
                "nullability-exception",
                SHELF,
                Some(11),
                &format!(
                    "{label} rootShelf insertion cycle requires its exact target, non-null relation type, nullable=true, RESTRICT/NO ACTION, and absent deferrability tuple"
                )
            ),
            row(
                "nullability-exception",
                SHELF,
                Some(11),
                &format!(
                    "{label} rootShelf nullability exception requires the exact non-null rootShelfId:string -> id:string component tuple"
                )
            ),
        ]
    );
    // A relation that departs from its entry reports its mismatch without the target an exception
    // would accept it by.
    assert_eq!(targets(&findings), [None, None, None]);

    let without_reason = check(&with_nullable_shelf(""), &[(SHELF, SHELF_DEPARTING)]);

    assert!(without_reason.iter().any(|finding| finding.3
        == format!(
            "{label} rootShelf requires its exact target, non-null relation type, nullable=true, RESTRICT/NO ACTION, and absent deferrability tuple"
        )));
}

const AUTHOR: &str = "src/modules/domain/author/entity.ts";
const AUTHOR_TEXT: &str = r#"import { Entity, PrimaryColumn } from "typeorm";

@Entity("author")
export class AuthorEntity {
 @PrimaryColumn({ type: "uuid" })
 id!: string;
}

@Entity("author_draft")
class AuthorDraftEntity {
 @PrimaryColumn({ type: "uuid" })
 id!: string;
}

export { AuthorDraftEntity as DraftEntity };
"#;
const NOVEL: &str = "src/modules/domain/novel/entity.ts";
const NOVEL_TEXT: &str = r#"import { Column, Entity, JoinColumn, ManyToOne, OneToOne, PrimaryColumn, type Relation } from "typeorm";

import { AuthorEntity, DraftEntity } from "../author/entity";

const target = () => AuthorEntity;

@Entity("novel")
export class NovelEntity {
 @PrimaryColumn({ type: "uuid" })
 id!: string;

 @Column({ type: "uuid" })
 selectedAuthorId!: string;

 @Column({ nullable: true, type: "uuid" })
 ownerAuthorId!: string | null;

 @Column({ type: "uuid" })
 draftId!: string;

 @JoinColumn({ foreignKeyConstraintName: "FK_novel_selected_author", name: "selectedAuthorId", referencedColumnName: "id" })
 @ManyToOne(() => AuthorEntity, { nullable: false, onDelete: "RESTRICT" })
 selectedAuthor!: Relation<AuthorEntity>;

 @JoinColumn({ foreignKeyConstraintName: "FK_novel_owner_author", name: "ownerAuthorId", referencedColumnName: "id" })
 @OneToOne(target, { nullable: true, onDelete: "SET NULL", onUpdate: "NO ACTION" })
 ownerAuthor?: AuthorEntity;

 @JoinColumn({ foreignKeyConstraintName: "FK_novel_draft", name: "draftId", referencedColumnName: "id" })
 @ManyToOne(() => DraftEntity, { nullable: true, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 draft!: Relation<DraftEntity>;

 @Column({ nullable: true, type: "uuid" })
 parentNovelId!: string | null;

 @JoinColumn({ foreignKeyConstraintName: "FK_novel_parent", name: "parentNovelId", referencedColumnName: "id" })
 @ManyToOne(() => NovelEntity, { nullable: true, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 parentNovel?: NovelEntity;
}
"#;

#[test]
fn relation_options_targets_types_names_and_nullability_are_reported() {
    let at = |line: u32, member: &str, message: &str| {
        format!("{NOVEL}:{line} NovelEntity.{member} {message}")
    };

    assert_eq!(
        check(UPPER, &[(AUTHOR, AUTHOR_TEXT), (NOVEL, NOVEL_TEXT)]),
        [
            row(
                "relation-options",
                NOVEL,
                Some(21),
                &at(
                    21,
                    "selectedAuthor",
                    "@ManyToOne options are missing required onUpdate"
                )
            ),
            row(
                "relation-name",
                NOVEL,
                Some(21),
                &at(
                    21,
                    "selectedAuthor",
                    "must use unambiguous target-resource name author; a sole relation must not carry mutable role prefix selected"
                )
            ),
            row(
                "relation-target",
                NOVEL,
                Some(25),
                &at(
                    25,
                    "ownerAuthor",
                    "@OneToOne target must be a direct zero-argument arrow to an entity identifier"
                )
            ),
            row(
                "relation-nullability",
                NOVEL,
                Some(29),
                &at(
                    29,
                    "draft",
                    "@ManyToOne option nullable=true must match relation property nullability=false"
                )
            ),
            row(
                "relation-target",
                NOVEL,
                Some(29),
                &at(
                    29,
                    "draft",
                    "target DraftEntity import \"../author/entity\" must resolve through explicit named exports to exactly one exported @Entity class DraftEntity"
                )
            ),
            row(
                "relation-type",
                NOVEL,
                Some(36),
                &at(
                    36,
                    "parentNovel",
                    "type must be exactly Relation<NovelEntity> or null | Relation<NovelEntity>"
                )
            ),
        ]
    );
}

const PUBLISHER: &str = "src/modules/domain/publisher/entity.ts";
const PUBLISHER_TEXT: &str = r#"import { Column, Entity, PrimaryColumn } from "typeorm";

@Entity("publisher")
export class PublisherEntity {
 @PrimaryColumn({ type: "uuid" })
 id!: string;

 @Column({ type: "integer" })
 rank!: number;
}
"#;
const EDITION: &str = "src/modules/domain/edition/entity.ts";
const EDITION_TEXT: &str = r#"import { Column, Entity, JoinColumn, ManyToOne, PrimaryColumn, type Relation } from "typeorm";

import { PublisherEntity } from "../publisher/entity";

@Entity("edition")
export class EditionEntity {
 @PrimaryColumn({ type: "uuid" })
 id!: string;

 @Column({ type: "uuid" })
 publisherId!: string;

 @Column({ type: "uuid" })
 otherId!: string;

 @Column({ type: "uuid" })
 imprintId!: string;

 @JoinColumn({ foreignKeyConstraintName: "FK_edition_publisher", name: "publisherId" })
 @ManyToOne(() => PublisherEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 publisher!: Relation<PublisherEntity>;

 @JoinColumn({ foreignKeyConstraintName: "FK_0123456789abcdef0123456789a", name: "otherId", referencedColumnName: "rank" })
 @ManyToOne(() => PublisherEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 imprint!: Relation<PublisherEntity>;

 @JoinColumn({ foreignKeyConstraintName: "edition_imprint_publisher", name: "missingId", referencedColumnName: "missing" })
 @ManyToOne(() => EditionEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 parent!: Relation<EditionEntity>;

 @JoinColumn({ foreignKeyConstraintName: "edition_imprint_publisher", name: "imprintId", referencedColumnName: "id" })
 @ManyToOne(() => EditionEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 previous!: Relation<EditionEntity>;
}
"#;

/// The findings of the edition tree, in message order.
fn edition_findings() -> Vec<Row> {
    let at = |line: u32, member: &str, message: &str| {
        format!("{EDITION}:{line} EditionEntity.{member} {message}")
    };
    let reused = "\"edition_imprint_publisher\"";

    vec![
        row(
            "foreign-key-reuse",
            EDITION,
            Some(27),
            &format!(
                "foreign key name {reused} is reused across owner relations: {EDITION}:27 EditionEntity.parent, {EDITION}:31 EditionEntity.previous"
            ),
        ),
        row(
            "join-column",
            EDITION,
            Some(19),
            &at(
                19,
                "publisher",
                "JoinColumn item 1 is missing referencedColumnName",
            ),
        ),
        row(
            "join-column-type",
            EDITION,
            Some(23),
            &at(
                23,
                "imprint",
                "JoinColumn local name \"otherId\" scalar type string must match referenced PublisherEntity.rank scalar type number",
            ),
        ),
        row(
            "foreign-key-name",
            EDITION,
            Some(23),
            &at(
                23,
                "imprint",
                "foreign key name \"FK_0123456789abcdef0123456789a\" is a TypeORM hash, not a semantic name",
            ),
        ),
        row(
            "join-column-name",
            EDITION,
            Some(23),
            &at(
                23,
                "imprint",
                "must join through its own persisted member-id column imprintId",
            ),
        ),
        row(
            "join-column-reference",
            EDITION,
            Some(27),
            &at(
                27,
                "parent",
                "JoinColumn local name \"missingId\" must resolve to exactly one same-class persisted TypeORM column",
            ),
        ),
        row(
            "join-column-reference",
            EDITION,
            Some(27),
            &at(
                27,
                "parent",
                "JoinColumn referenced name \"missing\" must resolve to exactly one persisted TypeORM column on EditionEntity",
            ),
        ),
        row(
            "foreign-key-name",
            EDITION,
            Some(27),
            &at(
                27,
                "parent",
                &format!("foreign key name {reused} must match {UPPER_PATTERN} in LIBRARY mode"),
            ),
        ),
        row(
            "foreign-key-name",
            EDITION,
            Some(31),
            &at(
                31,
                "previous",
                &format!("foreign key name {reused} must match {UPPER_PATTERN} in LIBRARY mode"),
            ),
        ),
    ]
}

#[test]
fn joins_their_references_types_and_foreign_key_names_are_reported() {
    assert_eq!(
        check(
            UPPER,
            &[(EDITION, EDITION_TEXT), (PUBLISHER, PUBLISHER_TEXT)]
        ),
        edition_findings()
    );
}

const LOAN: &str = "src/modules/domain/loan/entity/entity.ts";
const LOAN_TEXT: &str = r#"import * as orm from "typeorm";
import { Column as Persisted, JoinColumn } from "typeorm";

function ManyToOne(): PropertyDecorator {
 return () => undefined;
}

@orm.Entity("loan")
export class LoanEntity {
 @orm.PrimaryGeneratedColumn("uuid")
 id!: string;

 @Persisted(`uuid`)
 readerId!: string;

 @orm.OneToMany(() => LoanEntity, (loan) => loan.id)
 children!: Array<LoanEntity>;

 @JoinColumn({ foreignKeyConstraintName: "FK_loan_reader", name: "readerId", referencedColumnName: "id" })
 @ManyToOne()
 reader!: LoanEntity;
}
"#;

#[test]
fn banned_decorators_column_overloads_and_joins_without_an_owner_are_reported() {
    assert_eq!(
        check(UPPER, &[(LOAN, LOAN_TEXT)]),
        [
            row(
                "column-declaration",
                LOAN,
                Some(13),
                &format!(
                    "{LOAN}:13 LoanEntity.readerId @Column must use (), (\"type\"), ({{...}}), or (\"type\", {{...}})"
                )
            ),
            row(
                "prohibited-decorator",
                LOAN,
                Some(16),
                &format!("{LOAN}:16 uses prohibited TypeORM @OneToMany")
            ),
            row(
                "relation-owner",
                LOAN,
                Some(19),
                &format!(
                    "{LOAN}:19 LoanEntity.reader has a TypeORM @JoinColumn without an owning @ManyToOne or @OneToOne"
                )
            ),
        ]
    );
}

const WRITER: &str = "src/modules/domain/writer/entity.ts";
const WRITER_TEXT: &str = r#"import { Entity, PrimaryColumn } from "typeorm";

@Entity("writer")
export class WriterEntity {
 @PrimaryColumn({ nullable: false, type: "uuid" })
 id!: string;
}
"#;
const SERIES: &str = "src/modules/domain/series/entity.ts";
const SERIES_TEXT: &str = r#"import type * as orm from "typeorm";
import type { Relation as TypeormRelation } from "typeorm";

import { Column, Entity, JoinColumn, ManyToOne, PrimaryColumn } from "typeorm";

import { WriterEntity } from "../writer/entity";

@Entity("series")
export class SeriesEntity {
 @PrimaryColumn({ nullable: false, type: "uuid" })
 id!: string;

 @Column({ nullable: true, type: "uuid" })
 writerId!: null | string;

 @Column({ nullable: false, type: "uuid" })
 leadWriterId!: string;

 @Column({ nullable: false, type: "uuid" })
 guestWriterId!: string;

 @JoinColumn({ foreignKeyConstraintName: "FK_series_writer", name: "writerId", referencedColumnName: "id" })
 @ManyToOne(() => WriterEntity, { nullable: true, onDelete: "SET NULL", onUpdate: "NO ACTION" })
 writer!: WriterEntity | null;

 @JoinColumn({ foreignKeyConstraintName: "FK_series_lead_writer", name: "leadWriterId", referencedColumnName: "id" })
 @ManyToOne(() => WriterEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 leadWriter!: TypeormRelation<WriterEntity>;

 @JoinColumn({ foreignKeyConstraintName: "FK_series_guest_writer", name: "guestWriterId", referencedColumnName: "id" })
 @ManyToOne(() => WriterEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 guestWriter!: orm.Relation<WriterEntity>;

 @Column({ nullable: false, type: "uuid" })
 mirrorWriterId!: string;

 @Column({ nullable: false, type: "uuid" })
 listWriterId!: string;

 @JoinColumn({ foreignKeyConstraintName: "FK_series_mirror_writer", name: "mirrorWriterId", referencedColumnName: "id" })
 @ManyToOne(() => WriterEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 mirrorWriter!: TypeormRelation<SeriesEntity>;

 @JoinColumn({ foreignKeyConstraintName: "FK_series_list_writer", name: "listWriterId", referencedColumnName: "id" })
 @ManyToOne(() => WriterEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 listWriter!: Array<WriterEntity>;
}
"#;
const LOCAL_RELATION: &str = "src/modules/domain/relation.ts";
const LOCAL_RELATION_TEXT: &str = "export type Relation<T> = T;\n";
const CHAPTER: &str = "src/modules/domain/chapter/entity.ts";
const CHAPTER_TEXT: &str = r#"import type { Relation } from "../relation";

import { Column, Entity, JoinColumn, ManyToOne, PrimaryColumn } from "typeorm";

import { WriterEntity } from "../writer/entity";

@Entity("chapter")
export class ChapterEntity {
 @PrimaryColumn({ nullable: false, type: "uuid" })
 id!: string;

 @Column({ nullable: false, type: "uuid" })
 writerId!: string;

 @Column({ nullable: true, type: "uuid" })
 previousWriterId!: null | string;

 @Column({ nullable: false, type: "uuid" })
 seriesWriterId!: string;

 @JoinColumn({ foreignKeyConstraintName: "FK_chapter_writer", name: "writerId", referencedColumnName: "id" })
 @ManyToOne(() => WriterEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 writer!: WriterEntity;

 @JoinColumn({ foreignKeyConstraintName: "FK_chapter_previous_writer", name: "previousWriterId", referencedColumnName: "id" })
 @ManyToOne(() => WriterEntity, { nullable: true, onDelete: "SET NULL", onUpdate: "NO ACTION" })
 previousWriter!: null | WriterEntity;

 @JoinColumn({ foreignKeyConstraintName: "FK_chapter_series_writer", name: "seriesWriterId", referencedColumnName: "id" })
 @ManyToOne(() => WriterEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 seriesWriter!: Relation<WriterEntity>;
}
"#;
const REVIEW: &str = "src/modules/domain/review/entity.ts";
const REVIEW_TEXT: &str = r#"import { Column, Entity, JoinColumn, OneToOne, PrimaryColumn } from "typeorm";

import { WriterEntity } from "../writer/entity";

type Relation<T> = T;

@Entity("review")
export class ReviewEntity {
 @PrimaryColumn({ nullable: false, type: "uuid" })
 id!: string;

 @Column({ nullable: false, type: "uuid" })
 writerId!: string;

 @JoinColumn({ foreignKeyConstraintName: "FK_review_writer", name: "writerId", referencedColumnName: "id" })
 @OneToOne(() => WriterEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 writer!: Relation<WriterEntity>;
}
"#;
const WRAPPED: &str = r#"import type { Relation } from "typeorm";

import { Column, Entity, JoinColumn, ManyToOne, PrimaryColumn } from "typeorm";

import { WriterEntity } from "../writer/entity";

@Entity("essay")
export class EssayEntity {
 @PrimaryColumn({ nullable: false, type: "uuid" })
 id!: string;

 @Column({ nullable: false, type: "uuid" })
 writerId!: string;

 @Column({ nullable: true, type: "uuid" })
 previousWriterId!: null | string;

 @Column({ nullable: true, type: "uuid" })
 nextWriterId!: null | string;

 @JoinColumn({ foreignKeyConstraintName: "FK_essay_writer", name: "writerId", referencedColumnName: "id" })
 @ManyToOne(() => WriterEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 writer!: Relation<WriterEntity>;

 @JoinColumn({ foreignKeyConstraintName: "FK_essay_previous_writer", name: "previousWriterId", referencedColumnName: "id" })
 @ManyToOne(() => WriterEntity, { nullable: true, onDelete: "SET NULL", onUpdate: "NO ACTION" })
 previousWriter!: Relation<WriterEntity> | null;

 @JoinColumn({ foreignKeyConstraintName: "FK_essay_next_writer", name: "nextWriterId", referencedColumnName: "id" })
 @ManyToOne(() => WriterEntity, { nullable: true, onDelete: "SET NULL", onUpdate: "NO ACTION" })
 nextWriter!: null | Relation<WriterEntity>;
}
"#;

#[test]
fn only_the_unaliased_relation_of_the_package_wraps_a_target() {
    let wrap = |path: &str, line: u32, class: &str, member: &str| {
        row(
            "relation-wrapper",
            path,
            Some(line),
            &format!(
                "{path}:{line} {class}.{member} type must wrap WriterEntity as Relation<WriterEntity> imported by name from typeorm"
            ),
        )
    };
    let exact = |line: u32, member: &str| {
        row(
            "relation-type",
            SERIES,
            Some(line),
            &format!(
                "{SERIES}:{line} SeriesEntity.{member} type must be exactly Relation<WriterEntity> or null | Relation<WriterEntity>"
            ),
        )
    };

    assert_eq!(
        check(
            UPPER,
            &[
                (WRITER, WRITER_TEXT),
                (SERIES, SERIES_TEXT),
                (LOCAL_RELATION, LOCAL_RELATION_TEXT),
                (CHAPTER, CHAPTER_TEXT),
                (REVIEW, REVIEW_TEXT),
                ("src/modules/domain/essay/entity.ts", WRAPPED),
            ]
        ),
        [
            wrap(CHAPTER, 21, "ChapterEntity", "writer"),
            wrap(CHAPTER, 25, "ChapterEntity", "previousWriter"),
            wrap(CHAPTER, 29, "ChapterEntity", "seriesWriter"),
            wrap(REVIEW, 15, "ReviewEntity", "writer"),
            wrap(SERIES, 22, "SeriesEntity", "writer"),
            wrap(SERIES, 26, "SeriesEntity", "leadWriter"),
            wrap(SERIES, 30, "SeriesEntity", "guestWriter"),
            exact(40, "mirrorWriter"),
            exact(44, "listWriter"),
        ]
    );
}

const OPENING: &str = "src/modules/domain/opening/entity.ts";
const OPENING_TEXT: &str = r#"import { Entity, PrimaryColumn } from "typeorm";

@Entity("opening")
export class OpeningEntity {
 @PrimaryColumn({ type: "timestamptz" })
 startsAt!: Date;
}
"#;
const VISIT: &str = "src/modules/domain/visit/entity.ts";
const VISIT_TEXT: &str = r#"import { Column, Entity, JoinColumn, ManyToOne, PrimaryColumn, type Relation } from "typeorm";

import { OpeningEntity } from "../opening/entity";

@Entity("visit")
export class VisitEntity {
 @PrimaryColumn({ type: "uuid" })
 id!: string;

 @Column({ type: "timestamptz" })
 openingStartsAt!: Date;

 @JoinColumn({ foreignKeyConstraintName: "FK_visit_opening", name: "openingStartsAt", referencedColumnName: "startsAt" })
 @ManyToOne(() => OpeningEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 opening!: Relation<OpeningEntity>;
}
"#;

fn date_join() -> Row {
    row(
        "join-column-type",
        VISIT,
        Some(13),
        &format!(
            "{VISIT}:13 VisitEntity.opening JoinColumn local name \"openingStartsAt\" must declare one explicit closed TypeScript scalar type and must not be optional"
        ),
    )
}

#[test]
fn a_global_date_declaration_makes_date_no_scalar_and_constructor_decorators_are_dropped() {
    let opening = r#"import { Column, Entity, PrimaryColumn, RelationId } from "typeorm";

@Entity("opening")
export class OpeningEntity {
 @PrimaryColumn({ type: "timestamptz" })
 startsAt!: Date;

 @Column()
 ""!: string;

 @RelationId((opening: OpeningEntity) => opening.startsAt)
 constructor(@RelationId((opening: OpeningEntity) => opening.startsAt) startsAt: Date) {
  this.startsAt = startsAt;
 }
}
"#;

    assert_eq!(
        check(
            UPPER,
            &[
                (
                    "src/types/global.d.ts",
                    "interface Date {\n calendar: string;\n}\n"
                ),
                (OPENING, opening),
                (VISIT, VISIT_TEXT),
            ]
        ),
        [
            row(
                "prohibited-decorator",
                OPENING,
                Some(12),
                &format!("{OPENING}:12 uses prohibited TypeORM @RelationId")
            ),
            row(
                "column-declaration",
                OPENING,
                Some(8),
                &format!(
                    "{OPENING}:8 OpeningEntity. @Column must use a supported literal column overload"
                )
            ),
            date_join(),
        ]
    );
    assert_eq!(
        check(
            UPPER,
            &[
                (
                    "src/types/global.ts",
                    "export {};\n\ndeclare global {\n export interface Date {\n  calendar: string;\n }\n}\n"
                ),
                (OPENING, OPENING_TEXT),
                (VISIT, VISIT_TEXT),
            ]
        ),
        [date_join()]
    );
}

#[test]
fn a_date_declared_in_a_module_stays_local() {
    assert_eq!(
        check(
            UPPER,
            &[
                (
                    "src/types/meta.ts",
                    "const location = import.meta.url;\n\ninterface Date {\n location: typeof location;\n}\n"
                ),
                (
                    "src/types/local.ts",
                    "export {};\n\ninterface Date {\n calendar: string;\n}\n"
                ),
                (OPENING, OPENING_TEXT),
                (VISIT, VISIT_TEXT),
            ]
        ),
        []
    );
}

const READER: &str = "src/modules/domain/reader/entity.ts";
const READER_TEXT: &str = r#"import { Entity, PrimaryColumn } from "typeorm";

@Entity("reader")
export class ReaderEntity {
 @PrimaryColumn({ type: "uuid" })
 branchId!: string;

 @PrimaryColumn({ type: "varchar" })
 cardId!: string;
}
"#;
const BORROWING: &str = "src/modules/domain/borrowing/entity.ts";

fn borrowing(foreign_key: &str) -> String {
    format!(
        r#"import type {{ Relation }} from "typeorm";

import {{ Column, Entity, JoinColumn, ManyToOne, PrimaryColumn }} from "typeorm";

import {{ ReaderEntity }} from "../reader/entity";

@Entity("borrowing")
export class BorrowingEntity {{
 @PrimaryColumn({{ type: "uuid" }})
 id!: string;

 @Column({{ type: "uuid" }})
 branchId!: string;

 @Column({{ type: "varchar" }})
 readerCardId!: string;

 @JoinColumn([
  {{ foreignKeyConstraintName: "{foreign_key}", name: "branchId", referencedColumnName: "branchId" }},
  {{ name: "readerCardId", referencedColumnName: "cardId" }},
 ])
 @ManyToOne(() => ReaderEntity, {{ nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" }})
 reader!: Relation<ReaderEntity>;
}}
"#
    )
}

#[test]
fn the_case_option_chooses_the_foreign_key_spelling() {
    let lower = r#"{ "foreign-key-case": "lower", "foreign-key-label": "LOWER" }"#;
    let lower_name = borrowing("fk_borrowing_reader");
    let upper_name = borrowing("FK_borrowing_reader");

    assert_eq!(
        check(lower, &[(READER, READER_TEXT), (BORROWING, &lower_name)]),
        []
    );
    assert_eq!(
        check(lower, &[(READER, READER_TEXT), (BORROWING, &upper_name)]),
        [row(
            "foreign-key-name",
            BORROWING,
            Some(18),
            "src/modules/domain/borrowing/entity.ts:18 BorrowingEntity.reader foreign key name \"FK_borrowing_reader\" must match /^fk_[a-z][a-z0-9]*(?:_[a-z0-9]+)+$/u in LOWER mode"
        )]
    );
    assert_eq!(
        check(UPPER, &[(READER, READER_TEXT), (BORROWING, &upper_name)]),
        []
    );
}

#[test]
fn options_change_the_vocabulary_aliases_and_limits() {
    let options = r#"{ "foreign-key-case": "upper", "foreign-key-label": "LIBRARY",
        "banned-decorators": ["Column"], "relation-actions": ["RESTRICT"],
        "max-foreign-key-bytes": 12, "resolved-aliases": ["@library"] }"#;
    let plugin = r#"{ "layout": { "path-aliases": { "@library": "src/modules/domain" } } }"#;
    let novel = r#"import { Column, Entity, JoinColumn, ManyToOne, PrimaryColumn, type Relation } from "typeorm";

import { AuthorEntity } from "@library/author/entity";

@Entity("novel")
export class NovelEntity {
 @PrimaryColumn({ type: "uuid" })
 id!: string;

 @Column({ type: "uuid" })
 authorId!: string;

 @JoinColumn({ foreignKeyConstraintName: "FK_novel_author", name: "authorId", referencedColumnName: "id" })
 @ManyToOne(() => AuthorEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
 author!: Relation<AuthorEntity>;
}
"#;
    let findings = run(
        RULE,
        options,
        plugin,
        &[(AUTHOR, AUTHOR_TEXT), (NOVEL, novel)],
    )
    .expect("the rule runs");
    let label = format!("{NOVEL}:13 NovelEntity.author");

    assert_eq!(
        rows(&findings),
        [
            row(
                "prohibited-decorator",
                NOVEL,
                Some(10),
                &format!("{NOVEL}:10 uses prohibited TypeORM @Column")
            ),
            row(
                "relation-options",
                NOVEL,
                Some(13),
                &format!("{label} @ManyToOne option onUpdate must be one of RESTRICT")
            ),
            row(
                "foreign-key-name",
                NOVEL,
                Some(13),
                &format!(
                    "{label} foreign key name \"FK_novel_author\" must be shorter than 12 UTF-8 bytes"
                )
            ),
        ]
    );
}

#[test]
fn a_specifier_outside_the_followed_aliases_does_not_resolve() {
    let novel = NOVEL_TEXT.replace("../author/entity", "@modules/domain/author/entity");
    let plugin = r#"{ "layout": { "path-aliases": { "@library": "src/modules/domain", "@modules": "src/modules" } } }"#;
    let findings = run(
        RULE,
        r#"{ "foreign-key-case": "upper", "foreign-key-label": "LIBRARY", "resolved-aliases": ["@library"] }"#,
        plugin,
        &[(AUTHOR, AUTHOR_TEXT), (NOVEL, &novel)],
    )
    .expect("the rule runs");

    assert!(rows(&findings).iter().any(|finding| finding.3.ends_with(
        "target AuthorEntity import \"@modules/domain/author/entity\" must resolve to one repository TypeScript source file"
    )));
}

#[test]
fn the_run_stops_without_a_case_when_a_join_reaches_its_foreign_key_name() {
    assert!(
        error(
            RULE,
            "{}",
            &[(READER, READER_TEXT), (BORROWING, &borrowing("x"))]
        )
        .contains("foreign-key-case")
    );
    assert_eq!(check("{}", &[(LOAN, LOAN_TEXT)]).len(), 3);
}

#[test]
fn option_errors_name_the_option() {
    assert!(error(RULE, r#"{ "mode": "CORE" }"#, &[]).contains("mode"));
    assert!(error(RULE, r#"{ "foreign-key-case": "upper" }"#, &[]).contains("foreign-key-label"));
    assert!(error(RULE, r#"{ "resolved-aliases": ["@nowhere"] }"#, &[]).contains("@nowhere"));
    error(RULE, r#"{ "max-foreign-key-bytes": 0 }"#, &[]);
    assert!(
        error(
            RULE,
            &with_nullable_shelf("").replace("\"string\"", "\"text\""),
            &[]
        )
        .contains("text")
    );
    let _ = run_with(RULE, &Request::default(), &[]).expect("the defaults run");
}

#[test]
fn a_mismatch_on_a_relation_without_an_entry_has_no_target() {
    let findings = run(RULE, UPPER, "{}", &[(SHELF, SHELF_TEXT)]).expect("the rule runs");
    let mismatches: Vec<_> = findings
        .iter()
        .filter(|finding| finding.aspect == "relation-nullability")
        .collect();

    assert_eq!(mismatches.len(), 1, "{:?}", rows(&findings));
    assert_eq!(mismatches[0].target, None);
}

#[test]
fn files_in_skipped_folders_are_still_read() {
    let dist = "src/dist/shelf/entity/entity.ts";
    let generated = "src/generated/shelf/entity/entity.ts";

    assert_eq!(
        paths_with(
            RULE,
            UPPER,
            SKIP_DIST,
            &[(dist, SHELF_TEXT), (generated, SHELF_TEXT)]
        ),
        [dist, generated]
    );
}
