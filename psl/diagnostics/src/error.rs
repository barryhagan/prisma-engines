use colored::{ColoredString, Colorize};

use crate::{
    Span,
    pretty_print::{DiagnosticColorer, pretty_print},
};
use std::borrow::Cow;

#[derive(Debug, Clone)]
pub struct DatamodelError {
    span: Span,
    message: Cow<'static, str>,
}

impl DatamodelError {
    pub(crate) fn new(message: impl Into<Cow<'static, str>>, span: Span) -> Self {
        let message = message.into();
        DatamodelError { message, span }
    }

    pub fn new_static(message: &'static str, span: Span) -> Self {
        Self::new(message, span)
    }

    pub fn new_literal_parser_error(literal_type: &str, raw_value: &str, span: Span) -> DatamodelError {
        Self::new(
            format!("\"{raw_value}\" is not a valid value for {literal_type}."),
            span,
        )
    }

    pub fn new_named_env_val(span: Span) -> Self {
        Self::new(
            "The env function expects a singular, unnamed, string argument.".to_owned(),
            span,
        )
    }

    pub fn new_argument_not_found_error(argument_name: &str, span: Span) -> DatamodelError {
        Self::new(format!("Argument \"{argument_name}\" is missing."), span)
    }

    pub fn new_argument_count_mismatch_error(
        function_name: &str,
        required_count: usize,
        given_count: usize,
        span: Span,
    ) -> DatamodelError {
        let msg = format!("Function \"{function_name}\" takes {required_count} arguments, but received {given_count}.");
        Self::new(msg, span)
    }

    pub fn new_attribute_argument_not_found_error(
        argument_name: &str,
        attribute_name: &str,
        span: Span,
    ) -> DatamodelError {
        Self::new(
            format!("Argument \"{argument_name}\" is missing in attribute \"@{attribute_name}\"."),
            span,
        )
    }

    pub fn new_source_argument_not_found_error(argument_name: &str, source_name: &str, span: Span) -> DatamodelError {
        Self::new(
            format!("Argument \"{argument_name}\" is missing in data source block \"{source_name}\"."),
            span,
        )
    }

    pub fn new_generator_argument_not_found_error(
        argument_name: &str,
        generator_name: &str,
        span: Span,
    ) -> DatamodelError {
        Self::new(
            format!("Argument \"{argument_name}\" is missing in generator block \"{generator_name}\"."),
            span,
        )
    }

    pub fn new_attribute_validation_error(message: &str, attribute_name: &str, span: Span) -> DatamodelError {
        Self::new(format!("Error parsing attribute \"{attribute_name}\": {message}"), span)
    }

    pub fn new_duplicate_attribute_error(attribute_name: &str, span: Span) -> DatamodelError {
        let msg = format!("Attribute \"@{attribute_name}\" can only be defined once.");
        Self::new(msg, span)
    }

    pub fn new_incompatible_native_type(
        native_type: &str,
        field_type: &str,
        expected_types: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "Native type {native_type} is not compatible with declared field type {field_type}, expected field type {expected_types}.",
        );
        Self::new(msg, span)
    }

    pub fn new_invalid_native_type_argument(
        native_type: &str,
        got: &str,
        expected: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!("Invalid argument for type {native_type}: {got}. Allowed values: {expected}.");
        Self::new(msg, span)
    }

    pub fn new_invalid_prefix_for_native_types(
        given_prefix: &str,
        expected_prefix: &str,
        suggestion: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "The prefix {given_prefix} is invalid. It must be equal to the name of an existing datasource e.g. {expected_prefix}. Did you mean to use {suggestion}?"
        );
        DatamodelError::new(msg, span)
    }

    pub fn new_native_types_not_supported(connector_name: String, span: Span) -> DatamodelError {
        let msg = format!("Native types are not supported with {connector_name} connector");
        Self::new(msg, span)
    }

    pub fn new_reserved_scalar_type_error(type_name: &str, span: Span) -> DatamodelError {
        let msg = format!("\"{type_name}\" is a reserved scalar type name and cannot be used.");
        Self::new(msg, span)
    }

    pub fn new_duplicate_enum_database_name_error(span: Span) -> DatamodelError {
        let msg = "An enum with the same database name is already defined.";
        Self::new(msg, span)
    }

    pub fn new_duplicate_model_database_name_error(
        model_database_name: &str,
        existing_model_name: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "The model with database name \"{model_database_name}\" could not be defined because another model or view with this name exists: \"{existing_model_name}\""
        );
        Self::new(msg, span)
    }

    pub fn new_duplicate_view_database_name_error(
        model_database_name: &str,
        existing_model_name: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "The view with database name \"{model_database_name}\" could not be defined because another model or view with this name exists: \"{existing_model_name}\""
        );
        Self::new(msg, span)
    }

    pub fn new_duplicate_top_error(name: &str, top_type: &str, existing_top_type: &str, span: Span) -> DatamodelError {
        let msg = format!(
            "The {top_type} \"{name}\" cannot be defined because a {existing_top_type} with that name already exists.",
        );
        Self::new(msg, span)
    }

    pub fn new_duplicate_config_key_error(conf_block_name: &str, key_name: &str, span: Span) -> DatamodelError {
        let msg = format!("Key \"{key_name}\" is already defined in {conf_block_name}.");
        Self::new(msg, span)
    }

    pub fn new_duplicate_argument_error(arg_name: &str, span: Span) -> DatamodelError {
        Self::new(format!("Argument \"{arg_name}\" is already specified."), span)
    }

    pub fn new_unused_argument_error(span: Span) -> DatamodelError {
        Self::new("No such argument.", span)
    }

    pub fn new_duplicate_default_argument_error(arg_name: &str, span: Span) -> DatamodelError {
        let msg = format!("Argument \"{arg_name}\" is already specified as unnamed argument.");
        Self::new(msg, span)
    }

    pub fn new_duplicate_enum_value_error(enum_name: &str, value_name: &str, span: Span) -> DatamodelError {
        let msg = format!("Value \"{value_name}\" is already defined on enum \"{enum_name}\".",);
        Self::new(msg, span)
    }

    pub fn new_composite_type_duplicate_field_error(type_name: &str, field_name: &str, span: Span) -> DatamodelError {
        let msg = format!(
            "Field \"{}\" is already defined on {} \"{}\".",
            field_name, "composite type", type_name
        );
        Self::new(msg, span)
    }

    pub fn new_duplicate_field_error(
        model_name: &str,
        field_name: &str,
        container: &'static str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!("Field \"{field_name}\" is already defined on {container} \"{model_name}\".",);
        Self::new(msg, span)
    }

    pub fn new_scalar_list_fields_are_not_supported(
        container: &str,
        container_name: &str,
        field_name: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "Field \"{field_name}\" in {container} \"{container_name}\" can't be a list. The current connector does not support lists of primitive types."
        );
        Self::new(msg, span)
    }

    pub fn new_model_validation_error(
        message: &str,
        block_type: &'static str,
        model_name: &str,
        span: Span,
    ) -> DatamodelError {
        Self::new(
            format!("Error validating {block_type} \"{model_name}\": {message}"),
            span,
        )
    }

    pub fn new_composite_type_validation_error(message: &str, composite_type_name: &str, span: Span) -> DatamodelError {
        let msg = format!("Error validating composite type \"{composite_type_name}\": {message}",);
        Self::new(msg, span)
    }

    pub fn new_enum_validation_error(message: &str, enum_name: &str, span: Span) -> DatamodelError {
        Self::new(format!("Error validating enum `{enum_name}`: {message}"), span)
    }

    pub fn new_composite_type_field_validation_error(
        message: &str,
        composite_type_name: &str,
        field: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "Error validating field `{}` in {} `{}`: {}",
            field, "composite type", composite_type_name, message
        );
        Self::new(msg, span)
    }

    pub fn new_field_validation_error(
        message: &str,
        container_type: &str,
        container_name: &str,
        field: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!("Error validating field `{field}` in {container_type} `{container_name}`: {message}",);
        Self::new(msg, span)
    }

    pub fn new_source_validation_error(message: &str, source: &str, span: Span) -> DatamodelError {
        Self::new(format!("Error validating datasource `{source}`: {message}"), span)
    }

    pub fn new_validation_error(message: &str, span: Span) -> DatamodelError {
        Self::new(format!("Error validating: {message}"), span)
    }

    pub fn new_legacy_parser_error(message: impl Into<Cow<'static, str>>, span: Span) -> DatamodelError {
        Self::new(message.into(), span)
    }

    pub fn new_optional_argument_count_mismatch(
        native_type: &str,
        optional_count: usize,
        given_count: usize,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "Native type {native_type} takes {optional_count} optional arguments, but received {given_count}.",
        );

        DatamodelError::new(msg, span)
    }

    pub fn new_parser_error(expected_str: String, span: Span) -> DatamodelError {
        Self::new(format!("Unexpected token. Expected one of: {expected_str}"), span)
    }

    pub fn new_functional_evaluation_error(message: impl Into<Cow<'static, str>>, span: Span) -> DatamodelError {
        Self::new(message.into(), span)
    }

    pub fn new_environment_functional_evaluation_error(var_name: String, span: Span) -> DatamodelError {
        Self::new(format!("Environment variable not found: {var_name}."), span)
    }

    pub fn new_type_not_found_error(type_name: &str, span: Span) -> DatamodelError {
        let msg = format!(
            "Type \"{type_name}\" is neither a built-in type, nor refers to another model, composite type, or enum."
        );
        Self::new(msg, span)
    }

    pub fn new_type_for_case_not_found_error(type_name: &str, suggestion: &str, span: Span) -> DatamodelError {
        let msg = format!(
            "Type \"{type_name}\" is neither a built-in type, nor refers to another model, composite type, or enum. Did you mean \"{suggestion}\"?"
        );
        Self::new(msg, span)
    }

    pub fn new_scalar_type_not_found_error(type_name: &str, span: Span) -> DatamodelError {
        Self::new(format!("Type \"{type_name}\" is not a built-in type."), span)
    }

    pub fn new_attribute_not_known_error(attribute_name: &str, span: Span) -> DatamodelError {
        Self::new(format!("Attribute not known: \"@{attribute_name}\"."), span)
    }

    pub fn new_property_not_known_error(property_name: &str, span: Span) -> DatamodelError {
        Self::new(format!("Property not known: \"{property_name}\"."), span)
    }

    pub fn new_argument_not_known_error(property_name: &str, span: Span) -> DatamodelError {
        Self::new(format!("Argument not known: \"{property_name}\"."), span)
    }

    pub fn new_default_unknown_function(function_name: &str, span: Span) -> DatamodelError {
        DatamodelError::new(
            format!(
                "Unknown function in @default(): `{function_name}` is not known. You can read about the available functions here: https://pris.ly/d/attribute-functions"
            ),
            span,
        )
    }

    pub fn new_invalid_model_error(msg: &str, span: Span) -> DatamodelError {
        DatamodelError::new(format!("Invalid model: {msg}"), span)
    }

    pub fn new_datasource_provider_not_known_error(provider: &str, span: Span) -> DatamodelError {
        Self::new(format!("Datasource provider not known: \"{provider}\"."), span)
    }

    pub fn new_shadow_database_is_same_as_main_url_error(source_name: &str, span: Span) -> DatamodelError {
        let msg = format!(
            "shadowDatabaseUrl is the same as url for datasource \"{source_name}\". Please specify a different database as shadow database to avoid data loss."
        );
        Self::new(msg, span)
    }

    pub fn new_shadow_database_is_same_as_direct_url_error(source_name: &str, span: Span) -> DatamodelError {
        let msg = format!(
            "shadowDatabaseUrl is the same as directUrl for datasource \"{source_name}\". Please specify a different database as shadow database to avoid data loss."
        );
        Self::new(msg, span)
    }

    pub fn new_preview_feature_not_known_error(
        preview_feature: &str,
        expected_preview_features: String,
        span: Span,
    ) -> DatamodelError {
        let msg = format!(
            "The preview feature \"{preview_feature}\" is not known. Expected one of: {expected_preview_features}",
        );
        Self::new(msg, span)
    }

    pub fn new_value_parser_error(expected_type: &str, raw: &str, span: Span) -> DatamodelError {
        let msg = format!("Expected {expected_type}, but found {raw}.");
        Self::new(msg, span)
    }

    pub fn new_native_type_argument_count_mismatch_error(
        native_type: &str,
        required_count: usize,
        given_count: usize,
        span: Span,
    ) -> DatamodelError {
        let msg = format!("Native type {native_type} takes {required_count} arguments, but received {given_count}.");
        Self::new(msg, span)
    }

    pub fn new_native_type_name_unknown(connector_name: &str, native_type: &str, span: Span) -> DatamodelError {
        let msg = format!("Native type {native_type} is not supported for {connector_name} connector.");
        DatamodelError::new(msg, span)
    }

    pub fn new_native_type_parser_error(native_type: &str, span: Span) -> DatamodelError {
        let msg = format!("Invalid Native type {native_type}.");
        Self::new(msg, span)
    }

    pub fn new_type_mismatch_error(expected_type: &str, received_type: &str, raw: &str, span: Span) -> DatamodelError {
        let msg = format!("Expected a {expected_type} value, but received {received_type} value `{raw}`.");
        Self::new(msg, span)
    }

    pub fn new_schemas_array_empty_error(span: Span) -> DatamodelError {
        let msg = "If provided, the schemas array can not be empty.".to_string();
        Self::new(msg, span)
    }

    pub fn new_referential_integrity_and_relation_mode_cooccur_error(span: Span) -> DatamodelError {
        let msg = "The `referentialIntegrity` and `relationMode` attributes cannot be used together. Please use only `relationMode` instead.".to_string();
        Self::new(msg, span)
    }

    pub fn new_config_property_missing_value_error(
        property_name: &str,
        config_name: &str,
        config_kind: &str,
        span: Span,
    ) -> DatamodelError {
        let msg = format!("Property {property_name} in {config_kind} {config_name} needs to be assigned a value");
        Self::new(msg, span)
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn pretty_print(&self, f: &mut dyn std::io::Write, file_name: &str, text: &str) -> std::io::Result<()> {
        pretty_print(
            f,
            file_name,
            text,
            self.span(),
            self.message.as_ref(),
            &DatamodelErrorColorer {},
        )
    }
}

struct DatamodelErrorColorer {}

impl DiagnosticColorer for DatamodelErrorColorer {
    fn title(&self) -> &'static str {
        "error"
    }

    fn primary_color(&self, token: &'_ str) -> ColoredString {
        token.bright_red()
    }
}
