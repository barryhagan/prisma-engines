use super::{
    Rule,
    helpers::{Pair, parsing_catch_all},
    parse_expression::parse_expression,
};
use crate::ast;
use diagnostics::{Diagnostics, FileId};

pub(crate) fn parse_arguments_list(
    token: Pair<'_>,
    arguments: &mut ast::ArgumentsList,
    diagnostics: &mut Diagnostics,
    file_id: FileId,
) {
    debug_assert_eq!(token.as_rule(), Rule::arguments_list);
    for current in token.into_inner() {
        let current_span = current.as_span();
        match current.as_rule() {
            // This is a named arg.
            Rule::named_argument => arguments.arguments.push(parse_named_arg(current, diagnostics, file_id)),
            // This is an unnamed arg.
            Rule::expression => arguments.arguments.push(ast::Argument {
                name: None,
                value: parse_expression(current, diagnostics, file_id),
                span: ast::Span::from((file_id, current_span)),
            }),
            // This is an argument without a value.
            // It is not valid, but we parse it for autocompletion.
            Rule::empty_argument => {
                let name = current
                    .into_inner()
                    .find(|tok| tok.as_rule() == Rule::identifier)
                    .unwrap();
                arguments.empty_arguments.push(ast::EmptyArgument {
                    name: ast::Identifier::new(name, file_id),
                })
            }
            Rule::trailing_comma => {
                arguments.trailing_comma = Some((file_id, current.as_span()).into());
            }
            _ => parsing_catch_all(&current, "attribute arguments"),
        }
    }
}

fn parse_named_arg(pair: Pair<'_>, diagnostics: &mut Diagnostics, file_id: FileId) -> ast::Argument {
    debug_assert_eq!(pair.as_rule(), Rule::named_argument);
    let mut name: Option<ast::Identifier> = None;
    let mut argument: Option<ast::Expression> = None;
    let (pair_span, pair_str) = (pair.as_span(), pair.as_str());

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::identifier => name = Some(ast::Identifier::new(current, file_id)),
            Rule::expression => argument = Some(parse_expression(current, diagnostics, file_id)),
            _ => parsing_catch_all(&current, "attribute argument"),
        }
    }

    match (name, argument) {
        (Some(name), Some(value)) => ast::Argument {
            name: Some(name),
            value,
            span: ast::Span::from((file_id, pair_span)),
        },
        _ => panic!("Encountered impossible attribute arg during parsing: {pair_str:?}"),
    }
}
