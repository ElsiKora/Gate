//! `agents/skills`: the Agent Skills specification.

use crate::fixture::{Fixture, lines};

const SKILLS: &str = "docs/agents/skills";

#[test]
fn skills_follow_the_specification() {
    let long_name = "a".repeat(65);
    let long = format!(
        "---\nname: {long_name}\ndescription: {}\n---\n",
        "d".repeat(1025)
    );
    let long_path = format!("{SKILLS}/{long_name}/SKILL.md");
    let fixture = Fixture::new(&[
        (
            &format!("{SKILLS}/good/SKILL.md"),
            "---\nname: good\ndescription: Does a thing.\nlicense: MIT\ncompatibility: any\n\
             metadata: {owner: team}\nallowed-tools: Read\n---\n# Good\n",
        ),
        (&format!("{SKILLS}/good/extra/notes.md"), "# Notes\n"),
        (&format!("{SKILLS}/bare/README.md"), "# Bare\n"),
        (&format!("{SKILLS}/plain/SKILL.md"), "# Plain\n"),
        (&format!("{SKILLS}/broken/SKILL.md"), "---\nname: [\n---\n"),
        (&format!("{SKILLS}/list/SKILL.md"), "---\n- a\n---\n"),
        (
            &format!("{SKILLS}/noname/SKILL.md"),
            "---\ndescription: x\n---\n",
        ),
        (
            &format!("{SKILLS}/wrong/SKILL.md"),
            "---\nname: Other_Name\ndescription: \"   \"\ncompatibility: [a]\nmetadata: {key: [1]}\n\
             license: [MIT]\nallowed-tools: Read\nextra: x\n---\n",
        ),
        (&long_path, &long),
        (&format!("{SKILLS}/loose.md"), "# Not a skill\n"),
    ]);
    let found = lines(&fixture.check("skills", "{}").expect("runs"));
    let skill = |name: &str, rest: &str| format!("{SKILLS}/{name}{rest}");
    let yaml = skill("broken", "/SKILL.md:1 frontmatter is not valid YAML: ");
    assert!(
        found.iter().any(|line| line.starts_with(&yaml)),
        "{found:?}"
    );
    let found: Vec<String> = found
        .into_iter()
        .filter(|line| !line.starts_with(&yaml))
        .collect();
    assert_eq!(
        found,
        [
            format!("{long_path}:2 name is 1-64 lowercase letters, digits and single hyphens"),
            format!("{long_path}:3 description is required and has 1-1024 characters"),
            skill("bare", " a skill directory needs SKILL.md"),
            skill("list", "/SKILL.md:1 frontmatter is not a mapping"),
            skill("noname", "/SKILL.md:1 name is required"),
            skill("plain", "/SKILL.md:1 SKILL.md starts with YAML frontmatter"),
            skill("wrong", "/SKILL.md:8 `extra` is not an Agent Skills field"),
            skill(
                "wrong",
                "/SKILL.md:2 name is 1-64 lowercase letters, digits and single hyphens"
            ),
            skill(
                "wrong",
                "/SKILL.md:2 name `Other_Name` differs from the directory `wrong`"
            ),
            skill(
                "wrong",
                "/SKILL.md:3 description is required and has 1-1024 characters"
            ),
            skill(
                "wrong",
                "/SKILL.md:4 compatibility is a string of at most 500 characters"
            ),
            skill("wrong", "/SKILL.md:5 metadata maps strings to strings"),
            skill("wrong", "/SKILL.md:6 license is a string"),
        ]
    );
}
