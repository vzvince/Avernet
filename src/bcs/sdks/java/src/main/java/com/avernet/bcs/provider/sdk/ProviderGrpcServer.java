package com.avernet.bcs.provider.sdk;

import com.avernet.bcs.provider.demo.v1.InvokeRequest;
import com.avernet.bcs.provider.demo.v1.InvokeResponse;
import com.avernet.bcs.provider.demo.v1.ProviderDemoGrpc;
import io.grpc.Server;
import io.grpc.Status;
import io.grpc.netty.shaded.io.grpc.netty.NettyServerBuilder;
import io.grpc.stub.StreamObserver;

import java.io.IOException;
import java.net.InetSocketAddress;
import java.util.Objects;
import java.util.concurrent.TimeUnit;
import java.util.logging.Level;
import java.util.logging.Logger;

/** Owns the lifecycle of a standard grpc-java Provider demo server. */
public final class ProviderGrpcServer implements AutoCloseable {
    private final ProviderService service;
    private final String host;
    private final int port;
    private Server server;

    public ProviderGrpcServer(ProviderService service, String host, int port) {
        this.service = Objects.requireNonNull(service, "service");
        this.host = Objects.requireNonNull(host, "host");
        this.port = port;
    }

    public synchronized void start() throws IOException {
        if (server != null) {
            return;
        }
        server = NettyServerBuilder.forAddress(new InetSocketAddress(host, port))
                .addService(new ProviderDemoAdapter(service))
                .build()
                .start();
    }

    public synchronized int getPort() {
        if (server == null) {
            throw new IllegalStateException("provider server has not started");
        }
        return server.getPort();
    }

    public void blockUntilShutdown() throws InterruptedException {
        Server running;
        synchronized (this) {
            running = server;
        }
        if (running == null) {
            throw new IllegalStateException("provider server has not started");
        }
        running.awaitTermination();
    }

    @Override
    public synchronized void close() throws InterruptedException {
        if (server == null) {
            return;
        }
        Server running = server;
        server = null;
        running.shutdown();
        if (!running.awaitTermination(5, TimeUnit.SECONDS)) {
            running.shutdownNow();
            running.awaitTermination(5, TimeUnit.SECONDS);
        }
    }

    private static final class ProviderDemoAdapter
            extends ProviderDemoGrpc.ProviderDemoImplBase {
        private static final Logger LOGGER = Logger.getLogger(ProviderDemoAdapter.class.getName());
        private final ProviderService service;

        private ProviderDemoAdapter(ProviderService service) {
            this.service = service;
        }

        @Override
        public void invoke(
                InvokeRequest request,
                StreamObserver<InvokeResponse> responseObserver) {
            try {
                String message = service.invoke(request.getMessage());
                InvokeResponse response = InvokeResponse.newBuilder()
                        .setMessage(message)
                        .setImplementation(service.implementation())
                        .build();
                responseObserver.onNext(response);
                responseObserver.onCompleted();
            } catch (Exception error) {
                LOGGER.log(Level.SEVERE, "provider invocation failed", error);
                responseObserver.onError(Status.INTERNAL
                        .withDescription("provider invocation failed")
                        .asRuntimeException());
            }
        }
    }
}
