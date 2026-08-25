package com.avernet.bcs.provider.sdk;

import com.avernet.bcs.provider.demo.v1.InvokeRequest;
import com.avernet.bcs.provider.demo.v1.InvokeResponse;
import com.avernet.bcs.provider.demo.v1.ProviderDemoGrpc;
import io.grpc.ManagedChannel;
import io.grpc.ManagedChannelBuilder;
import io.grpc.Status;
import io.grpc.StatusRuntimeException;
import org.junit.jupiter.api.Test;

import java.util.concurrent.TimeUnit;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class ProviderGrpcServerTest {
    @Test
    void subclassReceivesGrpcInvocation() throws Exception {
        ProviderService service = new ProviderService() {
            @Override
            public String implementation() {
                return "java";
            }

            @Override
            public String invoke(String message) {
                return "java: " + message;
            }
        };

        ManagedChannel channel = null;
        try (ProviderGrpcServer server = new ProviderGrpcServer(service, "127.0.0.1", 0)) {
            server.start();
            channel = ManagedChannelBuilder.forAddress("127.0.0.1", server.getPort())
                    .usePlaintext()
                    .build();

            InvokeResponse response = ProviderDemoGrpc.newBlockingStub(channel)
                    .invoke(InvokeRequest.newBuilder().setMessage("hello").build());

            assertTrue(server.getPort() > 0);
            assertEquals("java: hello", response.getMessage());
            assertEquals("java", response.getImplementation());
        } finally {
            if (channel != null) {
                channel.shutdownNow().awaitTermination(5, TimeUnit.SECONDS);
            }
        }
    }

    @Test
    void handlerFailureIsRedactedFromGrpcResponse() throws Exception {
        ProviderService service = new ProviderService() {
            @Override
            public String implementation() {
                return "java";
            }

            @Override
            public String invoke(String message) {
                throw new RuntimeException("secret detail");
            }
        };

        ManagedChannel channel = null;
        try (ProviderGrpcServer server = new ProviderGrpcServer(service, "127.0.0.1", 0)) {
            server.start();
            channel = ManagedChannelBuilder.forAddress("127.0.0.1", server.getPort())
                    .usePlaintext()
                    .build();

            final ManagedChannel runningChannel = channel;
            StatusRuntimeException error = assertThrows(
                    StatusRuntimeException.class,
                    () -> ProviderDemoGrpc.newBlockingStub(runningChannel)
                            .invoke(InvokeRequest.newBuilder().setMessage("hello").build()));

            assertEquals(Status.Code.INTERNAL, error.getStatus().getCode());
            assertEquals("provider invocation failed", error.getStatus().getDescription());
            assertFalse(error.getMessage().contains("secret detail"));
        } finally {
            if (channel != null) {
                channel.shutdownNow().awaitTermination(5, TimeUnit.SECONDS);
            }
        }
    }
}
