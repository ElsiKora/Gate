//! The canonical spelling of wrapper constructions and raw Nest exception targets.

use nestjs::canonical::{canonical_construction, canonical_target};

fn canonical(text: &str) -> String {
    canonical_construction(text).expect("a construction")
}

#[test]
fn white_space_comments_trailing_commas_quotes_and_number_spellings_do_not_count() {
    let spellings = [
        "new BookException({ title: \"a\", pages: 16 })",
        "new BookException( {\n  title : 'a' , // the title\n  pages: 0x10 ,\n} )",
        "new  BookException /* why */ ({title:\"a\",pages:16.0,})",
    ];

    for spelling in spellings {
        assert_eq!(
            canonical(spelling),
            "new BookException({ title: \"a\", pages: 16 })",
            "{spelling}"
        );
    }
}

#[test]
fn parentheses_and_key_spellings_count() {
    let forms = [
        "new BookException(a)",
        "new BookException((a))",
        "new BookException({ a })",
        "new BookException({ a: a })",
        "new BookException({ \"a\": a })",
        "new BookException({ [a]: a })",
    ];
    let mut printed: Vec<String> = forms.iter().map(|form| canonical(form)).collect();

    assert_eq!(printed, forms);
    printed.sort();
    printed.dedup();
    assert_eq!(printed.len(), forms.len());
}

#[test]
fn a_construction_passed_first_to_a_call_keeps_only_the_callee() {
    assert_eq!(
        canonical("super.handle<T>(new BookException(error), 1)"),
        "super.handle(new BookException(error))"
    );
    assert_eq!(
        canonical("this . books [ 'handle' ] ( new BookException ( ) )"),
        "this.books[\"handle\"](new BookException())"
    );
    assert_eq!(canonical("new BookException"), "new BookException()");
}

#[test]
fn what_is_not_a_construction_has_no_canonical_form() {
    for text in [
        "",
        "BookException(a)",
        "new books.BookException(a)",
        "new BookException(a) + 1",
        "f(a, new BookException(a))",
        "books?.handle(new BookException(a))",
        "f((new BookException(a)))",
        "new BookException(",
        "new BookException(a); x",
    ] {
        assert_eq!(canonical_construction(text), None, "{text}");
    }
}

#[test]
fn expressions_and_types_print_on_one_line() {
    let cases = [
        (
            "new LibraryException({ code: ELibraryCode.INVALID, errors: errors.flatMap((error: BookError): Array<IBookIssue> => describe(error, \"\")) } satisfies ILibraryFailure)",
            "new LibraryException({ code: ELibraryCode.INVALID, errors: errors.flatMap((error: BookError): Array<IBookIssue> => describe(error, \"\")) } satisfies ILibraryFailure)",
        ),
        (
            "new LibraryException({errors:[{path:'',rule:shelf===null?ERule.EMPTY:ERule.FULL}]})",
            "new LibraryException({ errors: [{ path: \"\", rule: shelf === null ? ERule.EMPTY : ERule.FULL }] })",
        ),
        (
            "new LibraryException<A|B>(a?.b?.[c]!, ...rest, `x${y}z`, /a+/gi, -(-1), typeof a, 10n)",
            "new LibraryException<A | B>(a?.b?.[c]!, ...rest, `x${y}z`, /a+/gi, -(-1), typeof a, 10n)",
        ),
        (
            "new LibraryException(x as { readonly a?: string; b: number[] }, <T>y, [a,,b,], async x => await x)",
            "new LibraryException(x as { readonly a?: string; b: number[] }, <T>y, [a, , b], async (x) => await x)",
        ),
        (
            "new LibraryException(() => {\n  return 1; // one\n})",
            "new LibraryException(() => { return 1; })",
        ),
    ];

    for (text, expected) in cases {
        assert_eq!(canonical(text), expected, "{text}");
    }
}

#[test]
fn a_canonical_form_is_its_own_canonical_form() {
    for text in [
        "new BookException({ title: 'a', pages: [1, , 2,], ...rest })",
        "new BookException(- -a, + +b, !c, void 0, a ? (b, c) : d)",
        "new BookException(function () {  return 1; }, class {  a = 1; })",
        "new BookException(1e21, 0.000001, 1e-7, 1.5, \"a\\\"b\\n\")",
        "new BookException(({ a, b: [c, ...d], ...e }: T = {}, ...f: U[]) => a)",
    ] {
        let once = canonical(text);
        assert_eq!(canonical(&once), once, "{text}");
    }
}

#[test]
fn a_target_is_respelled_with_its_construction() {
    assert_eq!(
        canonical_target(
            "src/book.ts: BookService.read catch parameters shelf, author: new BookException( { shelf, author, } )"
        )
        .as_deref(),
        Some(
            "src/book.ts: BookService.read catch parameters shelf, author: new BookException({ shelf, author })"
        )
    );
    assert_eq!(
        canonical_target("src/book.ts: readBook: f(new BookException('a'))").as_deref(),
        Some("src/book.ts: readBook: f(new BookException(\"a\"))")
    );
}

#[test]
fn a_malformed_target_has_no_canonical_form() {
    for target in [
        "src/book.ts",
        "src/book.ts: new BookException()",
        ": readBook: new BookException()",
        "src/book.ts: read-book: new BookException()",
        "src/book.ts: .read: new BookException()",
        "src/book.ts: Book.read.more: new BookException()",
        "src/book.ts: readBook parameters: new BookException()",
        "src/book.ts: readBook parameters a, a: new BookException(a)",
        "src/book.ts: readBook parameters a-b: new BookException(a)",
        "src/book.ts: readBook: BookException()",
    ] {
        assert_eq!(canonical_target(target), None, "{target}");
    }
}
