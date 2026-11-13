use topcoat::{
    Result,
    context::Cx,
    view::{Attributes, Child, View, attributes, component, view},
};

use super::dialog::dialog;

/// A dialog that asks the user to respond to an important message.
///
/// Build its content with the dialog components and provide actions for answering or
/// cancelling. It uses `role="alertdialog"` and the browser's modal focus and
/// dismissal behavior, like [`dialog`].
///
/// Pass `aria-labelledby` and `aria-describedby` in `attrs`, pointing to the title and
/// description IDs. Other attributes are forwarded to the `<dialog>`.
///
/// ```ignore
/// view! {
///     alert_dialog(
///         id: "confirmation",
///         dialog_content(
///             dialog_header(
///                 dialog_title("Delete this workspace?")
///                 dialog_description("Its projects and deploys go with it.")
///             )
///             dialog_footer(
///                 <a href="/workspace" class=(button_variants(
///                     ButtonVariant::Ghost,
///                     ButtonSize::Md,
///                 ))>"Keep it"</a>
///                 button(variant: ButtonVariant::Destructive, "Delete")
///             )
///         )
///     )
/// }
/// ```
#[component]
pub async fn alert_dialog(
    cx: &Cx,
    /// ID used by the opener's `showModal()` call.
    id: &str,
    /// Extra attributes for the `<dialog>` element.
    #[default]
    attrs: Attributes,
    /// The alert dialog's content.
    #[default]
    child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        dialog(id: id, attrs: attributes! { role="alertdialog" (attrs) }, (child))
    })
}
