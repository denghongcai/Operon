use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tonic::{
    body::Body,
    codegen::{http, Service},
    transport::Channel,
    Status,
};

#[derive(Clone, Copy)]
pub(crate) struct RequestDeadline(pub Duration);

/// Enforces request deadlines through response-body completion, not only headers.
/// Requests without a deadline keep their long-lived streaming semantics.
#[derive(Clone, Debug)]
pub struct DeadlineChannel(pub(crate) Channel);

impl Service<http::Request<Body>> for DeadlineChannel {
    type Response = http::Response<Body>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.0.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, request: http::Request<Body>) -> Self::Future {
        let deadline = request
            .extensions()
            .get::<RequestDeadline>()
            .map(|timeout| tokio::time::Instant::now() + timeout.0);
        let response = self.0.call(request);
        Box::pin(async move {
            let response = if let Some(deadline) = deadline {
                tokio::time::timeout_at(deadline, response)
                    .await
                    .map_err(|_| Status::deadline_exceeded("gRPC request deadline exceeded"))??
            } else {
                response.await?
            };
            Ok(response.map(|body| match deadline {
                Some(deadline) => Body::new(DeadlineBody {
                    body,
                    timeout: Box::pin(tokio::time::sleep_until(deadline)),
                    finished: false,
                }),
                None => body,
            }))
        })
    }
}

struct DeadlineBody {
    body: Body,
    timeout: Pin<Box<tokio::time::Sleep>>,
    finished: bool,
}

impl http_body::Body for DeadlineBody {
    type Data = <Body as http_body::Body>::Data;
    type Error = Status;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        if self.finished {
            return Poll::Ready(None);
        }
        if self.timeout.as_mut().poll(cx).is_ready() {
            self.finished = true;
            self.body = Body::empty();
            return Poll::Ready(Some(Err(Status::deadline_exceeded(
                "gRPC response body deadline exceeded",
            ))));
        }
        let frame = Pin::new(&mut self.body).poll_frame(cx);
        if matches!(frame, Poll::Ready(None)) {
            self.finished = true;
        }
        frame
    }

    fn is_end_stream(&self) -> bool {
        self.finished || self.body.is_end_stream()
    }
    fn size_hint(&self) -> http_body::SizeHint {
        self.body.size_hint()
    }
}
