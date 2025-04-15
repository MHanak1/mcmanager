pub mod rejections {
    use warp::reject::Reject;

    #[derive(Debug)]
    pub struct NotFound;
    impl Reject for NotFound {}

    #[derive(Debug)]
    pub struct InternalServerError {
        pub error: String,
    }

    impl From<String> for InternalServerError {
        fn from(value: String) -> Self {
            Self { error: value }
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
}
