# linty

Simple, language-agnostic linter

## Table of Contents

- [linty](#linty)
  - [Table of Contents](#table-of-contents)
  - [Background](#background)
  - [Usage](#usage)
  - [Maintainers](#maintainers)

## Background

🚧 Linty is still under construction! The following is an ideal description following the principle of [Readme Driven Development](https://tom.preston-werner.com/2010/08/23/readme-driven-development.html).

Have you ever fixed a bug and wished you could warn other developers about that exact pitfall?

Do you wish there was an easy way to ban TODOs in your codebase?

Many language-specific linters are flexible enough to add new regex-based rules, but what if you want to ban a regex across your codebase, regardless of file?
What if you're using a language that doesn't have a fancy linter of its own?

That's where Linty comes in - it's a simple, language-agnostic linter to check for regex patterns across your codebase.

## Usage

Linty operates on a set of _rules_, each of which is a regex, a set of include globs, a set of exclude globs, and an associated error message and severity (warning or error).
Linty checks each regex against all the files it finds in the include glob but not the exclude glob, and warning or erroring as appropriate.

By default, Linty looks for a `.lintyconfig.json` file in your current directory.

```json
{
  "rules": [
    {
      "id": "WarnOnTodos",
      "message": "Are you sure you meant to leave a TODO?",
      "regex": "(TODO|todo)",
      "severity": "warning",
    },
    {
      "id": "NoXcxcInDocs",
      "message": "Don't leave xcxc in docs!",
      "regex": "(XCXC|xcxc)",
      "severity": "error",
      "includes": ["**/*.md"],
      "excludes": ["**/testing/*.md"]
    }
  ]
}
```

For each rule, it will apply the regex to each file found in the set of provided globs.
If no globs are provided, it will apply the regex to _all_ files recursively, starting with the current directory.
Files included in the `ignore` globs will be ignored.
If the `--pre-commit` option is specified, it will only apply the rules to files staged with git.
If explicit file paths are passed to Linty, it will only apply the rules to those files.
By default, Linty respects `.gitignore` files, but you can enable checking `.gitignore` files with `--no-ignore`.

If any `error` rules fail, Linty will report all failing rules and exit with exit code 1. If no `error` rules fail, Linty will exit with exit code 0.
If a `warn` rule fails, Linty will ask the user to confirm the warning manually.
If the `--error-on-warning` flag is used, warnings will instead be treated as errors; if the `--no-confirm-warning` flag is used, Linty will just print the warning, with no manual confirmation.

## Maintainers

[@rwblickhan](https://github.com/rwblickhan)