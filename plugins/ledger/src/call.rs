//! What the rules of one plugin call share ([`LedgerCall`]): one [`Catalog`] of the call's tree,
//! so each ledger file is read from the host and its fenced code blanked once per call whichever
//! rule asks first, and the records of the ledger collections ([`Ledger::read`]) read once.

use std::cell::RefCell;
use std::rc::Rc;

use gate_sdk::{Call, CallState, Tree};

use crate::catalog::Catalog;
use crate::records::Ledger;
use crate::settings::Settings;

/// The plugin's call state ([`CallState`]).
#[derive(Debug)]
pub struct LedgerCall;

impl CallState for LedgerCall {
    type Owner<'h> = Tree<'h>;
    type State<'o> = Shared<'o>;

    fn own<'h>(call: &Call<'h>) -> Tree<'h> {
        call.tree()
    }

    fn state<'o>(tree: &'o Tree<'_>) -> Shared<'o> {
        Shared {
            catalog: Catalog::new(*tree),
            ledger: RefCell::new(None),
        }
    }
}

/// The catalog and the records the rules of one call share.
#[derive(Debug)]
pub struct Shared<'o> {
    catalog: Catalog<'o>,
    ledger: RefCell<Option<Rc<Ledger>>>,
}

impl<'o> Shared<'o> {
    /// The call's catalog.
    #[must_use]
    pub fn catalog(&self) -> &Catalog<'o> {
        &self.catalog
    }

    /// The records of the ledger collections, read once per call: they depend only on the tree
    /// and the `[plugins.ledger] options`, which every rule of the call reads alike. A read that
    /// fails is not kept, so every rule that asks gets the error.
    ///
    /// # Errors
    /// As [`Ledger::read`].
    pub fn ledger(&self, settings: &Settings) -> gate_sdk::Result<Rc<Ledger>> {
        if let Some(known) = self.ledger.borrow().as_ref() {
            return Ok(Rc::clone(known));
        }
        let read = Rc::new(Ledger::read(&self.catalog, settings)?);
        *self.ledger.borrow_mut() = Some(Rc::clone(&read));
        Ok(read)
    }
}
