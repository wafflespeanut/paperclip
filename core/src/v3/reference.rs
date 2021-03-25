use super::v2;

pub(crate) fn invalid_referenceor<T>(message: String) -> openapiv3::ReferenceOr<T> {
    debug_assert!(false, "{}", message);
    openapiv3::ReferenceOr::ref_(&message)
}

impl<T> From<v2::Reference> for openapiv3::ReferenceOr<T> {
    fn from(v2: v2::Reference) -> Self {
        Self::from(&v2)
    }
}
impl<T> From<&v2::Reference> for openapiv3::ReferenceOr<T> {
    fn from(v2: &v2::Reference) -> Self {
        let reference = v2.reference.replace("definitions", "components/schemas");
        openapiv3::ReferenceOr::ref_(&reference)
    }
}
