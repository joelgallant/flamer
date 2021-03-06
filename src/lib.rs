#![feature(plugin_registrar, rustc_private, custom_attribute)]

extern crate rustc_plugin;
extern crate syntax;
extern crate rustc_data_structures;
extern crate smallvec;

use rustc_plugin::registry::Registry;
use syntax::ast::{Attribute, Block, Expr, ExprKind, Ident, Item, ItemKind, Mac,
                  MetaItem, Constness};
use syntax::fold::{self, Folder};
use syntax::ptr::P;
use syntax::source_map::{DUMMY_SP, Span};
use syntax::ext::base::{Annotatable, ExtCtxt, SyntaxExtension};
use syntax::ext::build::AstBuilder;
use syntax::feature_gate::AttributeType;
use syntax::symbol::Symbol;
use syntax::fold::ExpectOne;
use smallvec::SmallVec;

pub fn insert_flame_guard(cx: &mut ExtCtxt, _span: Span, _mi: &MetaItem,
                          a: Annotatable) -> Annotatable {
    match a {
        Annotatable::Item(i) => Annotatable::Item(
            Flamer { cx: cx, ident: i.ident }.fold_item(i).expect_one("expected exactly one item")),
        Annotatable::TraitItem(i) => Annotatable::TraitItem(
            i.map(|i| Flamer { cx, ident: i.ident }.fold_trait_item(i).expect_one("expected exactly one item"))),
        Annotatable::ImplItem(i) => Annotatable::ImplItem(
            i.map(|i| Flamer { cx, ident: i.ident }.fold_impl_item(i).expect_one("expected exactly one item"))),
        a => a
    }
}

struct Flamer<'a, 'cx: 'a> {
    ident: Ident,
    cx: &'a mut ExtCtxt<'cx>,
}

impl<'a, 'cx> Folder for Flamer<'a, 'cx> {
    fn fold_item(&mut self, item: P<Item>) -> SmallVec<[P<Item>; 1]> {
        if let ItemKind::Mac(_) = item.node {
            let expanded = self.cx.expander().fold_item(item);
            expanded.into_iter()
                    .flat_map(|i| fold::noop_fold_item(i, self).into_iter())
                    .collect()
        } else {
            fold::noop_fold_item(item, self)
        }
    }

    fn fold_item_simple(&mut self, i: Item) -> Item {
        fn is_flame_annotation(attr: &Attribute) -> bool {
            let name = attr.name();
            name == "flame" || name == "noflame"
        }
        // don't double-flame nested annotations
        if i.attrs.iter().any(is_flame_annotation) { return i; }

        // don't flame constant functions
        let is_const = if let ItemKind::Fn(_, ref header, ..) = i.node {
            header.constness.node == Constness::Const
        } else { false };
        if is_const { return i; }

        if let ItemKind::Mac(_) = i.node {
            return i;
        } else {
            self.ident = i.ident; // update in case of nested items
            fold::noop_fold_item_simple(i, self)
        }
    }

    fn fold_block(&mut self, block: P<Block>) -> P<Block> {
        block.map(|mut block| {
            let name = self.cx.expr_str(DUMMY_SP, self.ident.name);
            let ident = self.cx.ident_of("_name");
            let path = self.cx.path_global(DUMMY_SP,
                    vec![self.cx.ident_of("flame"), self.cx.ident_of("start_guard")]);
            let guard_path = self.cx.expr_path(path);
            let guard_call = self.cx.expr_call(DUMMY_SP, guard_path, vec![name]);
            let guard_stmt = self.cx.stmt_let(DUMMY_SP, false, ident, guard_call);
            block.stmts.insert(0, guard_stmt);
            block
        })
    }

    fn fold_expr(&mut self, expr: P<Expr>) -> P<Expr> {
        if let ExprKind::Mac(_) = expr.node {
            self.cx.expander().fold_expr(expr)
                              .map(|e| fold::noop_fold_expr(e, self))
        } else {
            expr
        }
    }

    fn fold_mac(&mut self, mac: Mac) -> Mac {
        mac
    }
}

#[plugin_registrar]
pub fn plugin_registrar(reg: &mut Registry) {
    reg.register_syntax_extension(Symbol::intern("flame"),
        SyntaxExtension::MultiModifier(Box::new(insert_flame_guard)));
    reg.register_attribute(String::from("noflame"), AttributeType::Whitelisted);
}
