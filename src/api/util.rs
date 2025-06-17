pub mod rejections {
    use std::error::Error;
    use warp::reject::Reject;

    #[derive(Debug)]
    pub struct NotFound;
    impl Reject for NotFound {}

    #[derive(Debug)]
    pub struct MethodNotAllowed;
    impl Reject for MethodNotAllowed {}

    #[derive(Debug)]
    pub struct InternalServerError {
        pub error: String,
    }

    impl<T: ToString> From<T> for InternalServerError {
        fn from(value: T) -> Self {
            Self {
                error: value.to_string(),
            }
        }
    }

    impl Reject for InternalServerError {}

    #[derive(Debug)]
    pub struct InvalidBearerToken;
    impl Reject for InvalidBearerToken {}

    #[derive(Debug)]
    pub struct Unauthorized;
    impl Reject for Unauthorized {}

    #[derive(Debug)]
    pub struct BadRequest;
    impl Reject for BadRequest {}

    #[derive(Debug)]
    pub struct NotImplemented;
    impl Reject for NotImplemented {}

    #[derive(Debug)]
    pub struct Conflict;
    impl Reject for Conflict {}
}
