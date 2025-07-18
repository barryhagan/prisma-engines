use darling::{ToTokens, ast::NestedMeta};
use quote::{TokenStreamExt, quote};
use std::{
    collections::{HashMap, hash_map},
    ops::{Deref, DerefMut},
};

#[derive(Debug, Default)]
pub struct NestedAttrMap {
    inner: HashMap<String, NestedMeta>,
}

impl Deref for NestedAttrMap {
    type Target = HashMap<String, NestedMeta>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for NestedAttrMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl NestedAttrMap {
    /// Merges this attr map with the incoming one.
    /// Rules:
    /// - If `self` already contains a key, do not overwrite.
    /// - If `self` contains `"only"` or `"exclude"`, then neither of the incoming
    ///   `"only"` or `"exclude"` are merged, because the test overwrites the connectors to test.
    pub fn merge(mut self, other: &Self) -> Self {
        let self_has_connector = self.contains_key("only") || self.contains_key("exclude");

        for (k, v) in other.iter() {
            let incoming_is_connector = k == "only" || k == "exclude";
            let allow_insert = !(self_has_connector && incoming_is_connector);

            match self.inner.entry(k.clone()) {
                hash_map::Entry::Vacant(vacant) if allow_insert => {
                    vacant.insert(v.clone());
                }
                _ => {}
            }
        }

        self
    }

    /// Converts a list of [`NestedMeta`] into a [`NestedAttrMap`].
    pub fn from_nested_meta<T: IntoIterator<Item = NestedMeta>>(args: T) -> Result<Self, syn::Error> {
        let map = args
            .into_iter()
            .map(|attr| match attr {
                NestedMeta::Meta(ref meta) => {
                    let ident = meta.path().get_ident().unwrap().to_string();
                    Ok((ident, attr))
                }
                NestedMeta::Lit(_) => Err(syn::Error::new_spanned(
                    attr,
                    "Unexpected literal encountered in NestedAttrMap parsing.",
                )),
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

        Ok(Self { inner: map })
    }
}

impl ToTokens for NestedAttrMap {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let metas: Vec<_> = self.inner.values().collect();
        tokens.append_all(quote! { #(#metas),* });
    }

    fn to_token_stream(&self) -> proc_macro2::TokenStream {
        let mut tokens = proc_macro2::TokenStream::new();
        self.to_tokens(&mut tokens);
        tokens
    }

    fn into_token_stream(self) -> proc_macro2::TokenStream
    where
        Self: Sized,
    {
        self.to_token_stream()
    }
}
