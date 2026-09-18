//! The vocabulary a capability grant's resource scope is written in.
//!
//! # The defect this exists to fix
//!
//! `resource_scope` was a `String` checked only for being non-blank, and every
//! place that asked for authorization invented its own way of naming the thing
//! it was asking about. By the time this module was written there were five
//! vocabularies in one system:
//!
//! | where | what it said |
//! |---|---|
//! | HTTP router | `/v1/memories/0198…` |
//! | memory purge | `memory://0198…` |
//! | health disposition | `finding://0198…` |
//! | run worker | `run:0198…` |
//! | MCP server | `memory_id:0198…`, or `workspace` |
//! | grant policy engine | `unscoped` |
//!
//! Two things follow from that, and both were live.
//!
//! **One resource had three names.** The same memory is `/v1/memories/0198…`
//! coming through HTTP, `memory://0198…` when it is purged, and
//! `memory_id:0198…` through MCP. A grant written in one of those authorises
//! nothing in the other two, so an operator who wants to permit a purge has to
//! write the resource down twice in two spellings and has no way to find that
//! out except by being denied.
//!
//! **Two of those spellings could not express a workspace-wide grant at all.**
//! [`scope_is_within`](super::scope_is_within) treats `/` as the boundary
//! between a scope and what is inside it, which is why `memory://` contains
//! `memory://0198…`. `run:` does not contain `run:0198…` — there is no `/` for
//! the boundary rule to find. "May act on any run in this workspace" was
//! therefore unwriteable, and the operator who tried it got a denial that named
//! `ResourceMismatch` and nothing else.
//!
//! # What is checked, and when
//!
//! A scope is now parsed when a grant is issued and when an authorization is
//! requested. A grant that could never match anything is refused at the moment
//! somebody writes it, naming the vocabulary — rather than being stored, looking
//! plausible in a listing, and denying every request forever.
//!
//! The kinds this system names things with are a **closed set**, and the shapes
//! are fixed: `run:0198…`, `memory_id:0198…`, `unscoped` and `workspace` are all
//! refused, because none of them is anything anything asks about.
//!
//! # What the closed set does not reach
//!
//! An external effect acts on somebody else's resource — a payments adapter
//! charges `customer://42/card` — and this system does not know which
//! namespaces an adapter speaks. External schemes are therefore open, and the
//! consequence is exact: `memroy://0198…` is refused as an internal scope only
//! because `memroy` is not a kind, and is then **admitted as an external
//! target**. A misspelled internal kind survives as a grant that matches
//! nothing.
//!
//! What still catches it is the check on the other side: an authorization
//! *request* is parsed too, and every request this system makes is internal. A
//! caller cannot ask about `memroy://` without the request being refused, so a
//! misspelled grant is unreachable rather than dangerous — it is a permission
//! that does nothing, not a permission that does something unintended.
//!
//! # What this deliberately does not unify
//!
//! Surface scopes (`/v1/memories`) remain their own shape. A grant on an HTTP
//! path is about which door a caller may knock on; a grant on `memory://` is
//! about which resource may be touched once inside. Those are two different
//! questions and collapsing them here would silently widen one of them. That
//! the router asks the first and the purge asks the second — so a purge needs
//! two grants — is a real design question this module records rather than
//! answers.

use super::scope_is_within;
use crate::DomainError;
use std::fmt;
use std::str::FromStr;

/// The kinds of thing a capability can be about.
///
/// Closed on purpose. Every capability in [`Capability`](super::Capability) is
/// about one of these, and a scope naming anything else is a mistake rather
/// than a narrower permission.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResourceKind {
    Memory,
    Event,
    Context,
    Agent,
    Skill,
    Workflow,
    Execution,
    Run,
    Model,
    Provider,
    Evaluation,
    Learning,
    Audit,
    Export,
    Finding,
    Capability,
    Workspace,
}

impl ResourceKind {
    /// Every kind, so callers can enumerate the vocabulary instead of guessing
    /// at it — including the error message that tells an operator what they may
    /// have meant.
    pub const ALL: &'static [Self] = &[
        Self::Memory,
        Self::Event,
        Self::Context,
        Self::Agent,
        Self::Skill,
        Self::Workflow,
        Self::Execution,
        Self::Run,
        Self::Model,
        Self::Provider,
        Self::Evaluation,
        Self::Learning,
        Self::Audit,
        Self::Export,
        Self::Finding,
        Self::Capability,
        Self::Workspace,
    ];

    pub const fn scheme(&self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Event => "event",
            Self::Context => "context",
            Self::Agent => "agent",
            Self::Skill => "skill",
            Self::Workflow => "workflow",
            Self::Execution => "execution",
            Self::Run => "run",
            Self::Model => "model",
            Self::Provider => "provider",
            Self::Evaluation => "evaluation",
            Self::Learning => "learning",
            Self::Audit => "audit",
            Self::Export => "export",
            Self::Finding => "finding",
            Self::Capability => "capability",
            Self::Workspace => "workspace",
        }
    }
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.scheme())
    }
}

impl FromStr for ResourceKind {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.scheme() == value)
            .ok_or_else(|| {
                let known = Self::ALL
                    .iter()
                    .map(|kind| kind.scheme())
                    .collect::<Vec<_>>()
                    .join(", ");
                DomainError::InvalidArgument(format!(
                    "resource scope names the unknown kind `{value}`, so it would match nothing \
                     that is ever checked; the kinds are: {known}"
                ))
            })
    }
}

/// A parsed resource scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResourceScope {
    /// Every resource of a kind in the workspace: `memory://`.
    ///
    /// The trailing `//` is not decoration. It is what makes the scope a
    /// container under the boundary rule, and writing `memory:` instead would
    /// produce a scope that matches only itself.
    EveryOfKind { kind: ResourceKind },
    /// One resource: `memory://0198…`.
    One { kind: ResourceKind, id: String },
    /// An HTTP surface, by path: `/v1/memories`.
    Surface { path: String },
    /// Somewhere outside this system: `https://hooks.example.com/x`,
    /// `customer://42/card`.
    ///
    /// External effects act on somebody else's resources, and the provider's
    /// own name for the thing is the honest name of what is being permitted —
    /// a payments adapter charges a customer's card, not a URL. The scheme is
    /// therefore **not** constrained here: this system does not know which
    /// namespaces an adapter speaks, and a closed list would mean every new
    /// adapter needed a domain change to be grantable.
    ///
    /// `https://hooks.example.com/` and `customer://` each contain everything
    /// beneath them under the same boundary rule, so "any customer" is
    /// writeable.
    External { scheme: String, target: String },
}

impl ResourceScope {
    /// Every resource of a kind.
    pub const fn every(kind: ResourceKind) -> Self {
        Self::EveryOfKind { kind }
    }

    /// One resource, named.
    pub fn one(kind: ResourceKind, id: impl fmt::Display) -> Self {
        Self::One {
            kind,
            id: id.to_string(),
        }
    }

    /// The workspace itself — what an authorization that is not about any
    /// particular resource is asking about.
    ///
    /// This replaces the bare words `unscoped` and `workspace`, which were not
    /// in any vocabulary and could only ever be matched by a grant that spelled
    /// them identically.
    pub const fn workspace() -> Self {
        Self::EveryOfKind {
            kind: ResourceKind::Workspace,
        }
    }

    /// Parse a scope, refusing anything that could not match.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(DomainError::InvalidArgument(
                "capability grant resource scope must not be empty".into(),
            ));
        }
        if trimmed.len() != value.len() {
            return Err(DomainError::InvalidArgument(format!(
                "resource scope `{value}` is surrounded by whitespace, and would not match the \
                 trimmed form anything asks about"
            )));
        }
        if trimmed.chars().any(char::is_whitespace) {
            return Err(DomainError::InvalidArgument(format!(
                "resource scope `{value}` contains whitespace"
            )));
        }
        if trimmed.contains('*') {
            return Err(DomainError::InvalidArgument(format!(
                "resource scope `{value}` contains `*`, which is refused everywhere it is \
                 compared rather than treated as a wildcard; name the container instead, as in \
                 `memory://`"
            )));
        }

        if let Some(path) = trimmed.strip_prefix('/') {
            if path.is_empty() {
                return Err(DomainError::InvalidArgument(
                    "resource scope `/` is the whole HTTP surface, which is not a scope anybody \
                     writes deliberately"
                        .into(),
                ));
            }
            return Ok(Self::Surface {
                path: trimmed.to_owned(),
            });
        }

        let Some((scheme, rest)) = trimmed.split_once("://") else {
            return Err(DomainError::InvalidArgument(format!(
                "resource scope `{value}` is in no vocabulary this system checks; a scope is \
                 `<kind>://` for every resource of a kind, `<kind>://<id>` for one, `/<path>` \
                 for an HTTP surface, or `<scheme>://<target>` for something at a provider"
            )));
        };

        if scheme.is_empty() {
            return Err(DomainError::InvalidArgument(format!(
                "resource scope `{value}` names no kind before `://`"
            )));
        }

        if let Ok(kind) = scheme.parse::<ResourceKind>() {
            return Ok(if rest.is_empty() {
                Self::EveryOfKind { kind }
            } else {
                Self::One {
                    kind,
                    id: rest.to_owned(),
                }
            });
        }

        // Not one of this system's kinds, so it names something outside it. The
        // only thing checkable here is that it names *something*: a bare
        // `https://` would be a grant over every external target an adapter
        // could ever reach.
        if rest.is_empty() {
            return Err(DomainError::InvalidArgument(format!(
                "resource scope `{value}` names no target after `{scheme}://`, so it would \
                 contain everything that scheme can reach"
            )));
        }
        Ok(Self::External {
            scheme: scheme.to_owned(),
            target: rest.to_owned(),
        })
    }

    /// Whether this scope names something inside this system rather than at a
    /// provider.
    ///
    /// The distinction is not cosmetic: an internal scope is checked against a
    /// closed set of kinds, and an external one cannot be, so a caller that
    /// wants the stronger guarantee has to ask which it got.
    pub const fn is_internal(&self) -> bool {
        matches!(
            self,
            Self::EveryOfKind { .. } | Self::One { .. } | Self::Surface { .. }
        )
    }

    /// Whether a grant written as `self` covers `requested`.
    ///
    /// Delegates to the boundary rule so there is one answer to "is this inside
    /// that", not two that can drift apart.
    pub fn covers(&self, requested: &Self) -> bool {
        scope_is_within(&requested.to_string(), &self.to_string())
    }
}

impl fmt::Display for ResourceScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EveryOfKind { kind } => write!(f, "{kind}://"),
            Self::One { kind, id } => write!(f, "{kind}://{id}"),
            Self::Surface { path } => f.write_str(path),
            Self::External { scheme, target } => write!(f, "{scheme}://{target}"),
        }
    }
}

impl FromStr for ResourceScope {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_container_holds_its_resources() {
        let every = ResourceScope::parse("memory://").expect("a container scope");
        let one = ResourceScope::parse("memory://0198abc").expect("one memory");
        assert!(every.covers(&one));
        assert!(!one.covers(&every));
    }

    #[test]
    fn the_run_scope_that_could_not_contain_a_run_now_can() {
        // The defect: `run:0198abc` was the shape the worker asked with, and
        // `run:` could not contain it, so no workspace-wide run grant existed.
        assert!(ResourceScope::parse("run:0198abc").is_err());
        let every = ResourceScope::parse("run://").expect("every run");
        let one = ResourceScope::parse("run://0198abc").expect("one run");
        assert!(every.covers(&one));
    }

    #[test]
    fn a_misspelled_kind_is_not_an_internal_scope() {
        // It parses — as an external target, because nothing here knows which
        // namespaces a provider adapter speaks. What it is not is internal, and
        // no request this system makes is external, so the grant is unreachable
        // rather than unintentionally wide.
        let typo = ResourceScope::parse("memroy://0198abc").expect("an unknown scheme is external");
        assert!(!typo.is_internal());
        let real = ResourceScope::parse("memory://0198abc").expect("a memory");
        assert!(real.is_internal());
        assert!(!typo.covers(&real), "a typo'd scope reached a real memory");
        assert!(!real.covers(&typo));
    }

    #[test]
    fn an_unknown_kind_without_a_scheme_names_the_vocabulary() {
        let error = ResourceScope::parse("memroy:0198abc").expect_err("no `://`, so no scope");
        assert!(error.to_string().contains("<kind>://"), "{error}");
    }

    #[test]
    fn a_provider_namespace_is_grantable() {
        let every = ResourceScope::parse("customer://").expect_err("no target named");
        assert!(every.to_string().contains("everything"), "{every}");
        let all_cards = ResourceScope::parse("customer://42").expect("one customer");
        let card = ResourceScope::parse("customer://42/card").expect("a card");
        assert!(all_cards.covers(&card));
        assert!(!all_cards.covers(&ResourceScope::parse("customer://43/card").expect("another")));
    }

    #[test]
    fn the_old_bare_words_are_refused() {
        // Every one of these was a live scope in this system, and none of them
        // could be matched by a grant that did not spell it identically.
        for bare in ["unscoped", "workspace", "memory_id:0198abc", "run:0198abc"] {
            assert!(
                ResourceScope::parse(bare).is_err(),
                "`{bare}` parsed, and it is in no vocabulary anything checks"
            );
        }
    }

    #[test]
    fn a_kind_container_does_not_hold_another_kind() {
        let memories = ResourceScope::parse("memory://").expect("memories");
        let runs = ResourceScope::parse("run://0198abc").expect("a run");
        assert!(!memories.covers(&runs));
    }

    #[test]
    fn one_resource_does_not_contain_a_longer_named_one() {
        let short = ResourceScope::parse("memory://ab").expect("a memory");
        let long = ResourceScope::parse("memory://abc").expect("a different memory");
        assert!(!short.covers(&long));
    }

    #[test]
    fn surfaces_nest_by_path_segment() {
        let v1 = ResourceScope::parse("/v1").expect("the v1 surface");
        let memories = ResourceScope::parse("/v1/memories").expect("a v1 surface");
        let admin = ResourceScope::parse("/v1-admin").expect("a different surface");
        assert!(v1.covers(&memories));
        assert!(!v1.covers(&admin));
    }

    #[test]
    fn a_surface_grant_does_not_cover_a_resource() {
        // The two axes do not collapse into each other: permitting a door is
        // not permitting everything behind it.
        let surface = ResourceScope::parse("/v1/memories").expect("a surface");
        let resource = ResourceScope::parse("memory://0198abc").expect("a memory");
        assert!(!surface.covers(&resource));
        assert!(!resource.covers(&surface));
    }

    #[test]
    fn an_external_host_contains_its_paths() {
        let host = ResourceScope::parse("https://hooks.example.com/").expect("a host");
        let hook = ResourceScope::parse("https://hooks.example.com/deploy").expect("a hook");
        let other = ResourceScope::parse("https://hooks.example.com.evil.test/").expect("another");
        assert!(host.covers(&hook));
        assert!(
            !host.covers(&other),
            "a host grant reached a lookalike domain"
        );
    }

    #[test]
    fn a_scheme_with_no_host_is_refused() {
        assert!(ResourceScope::parse("https://").is_err());
    }

    #[test]
    fn the_whole_surface_is_not_a_scope() {
        assert!(ResourceScope::parse("/").is_err());
    }

    #[test]
    fn wildcards_are_refused_where_they_are_written() {
        // `scope_is_within` refuses `*` on both sides, so a grant containing one
        // matches nothing. Refusing it here says so at the point of writing.
        assert!(ResourceScope::parse("memory://*").is_err());
    }

    #[test]
    fn constructors_and_parsing_agree() {
        assert_eq!(
            ResourceScope::every(ResourceKind::Memory).to_string(),
            "memory://"
        );
        assert_eq!(
            ResourceScope::one(ResourceKind::Run, "0198abc").to_string(),
            "run://0198abc"
        );
        assert_eq!(ResourceScope::workspace().to_string(), "workspace://");
        for kind in ResourceKind::ALL {
            let scope = ResourceScope::every(*kind);
            assert_eq!(
                ResourceScope::parse(&scope.to_string()).expect("a constructed scope parses"),
                scope
            );
        }
    }
}
