pub mod data_types {
    use serde::Deserialize;
    #[derive(Debug, Deserialize)]
    pub struct LoginCredentials {
        pub username: String,
        pub password: String,
    }
}

pub mod rejections {
    use warp::reject::Reject;

    #[derive(Debug)]
    pub struct InternalServerError;
    impl Reject for InternalServerError {}

    #[derive(Debug)]
    pub struct InvalidBearerToken;
    impl Reject for InvalidBearerToken {}

    #[derive(Debug)]
    pub struct Unauthorized;
    impl Reject for Unauthorized {}
}