package com.avernet.bcs.provider.sdk.example;

import com.avernet.bcs.provider.sdk.ProviderGrpcServer;
import com.avernet.bcs.provider.sdk.ProviderService;

/** Executable example of a Provider SDK subclass. */
public final class EchoServer {
    private EchoServer() {
    }

    public static void main(String[] args) throws Exception {
        Arguments arguments = Arguments.parse(args);
        final ProviderGrpcServer server = new ProviderGrpcServer(
                new EchoProvider(), arguments.host, arguments.port);
        server.start();
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {
            try {
                server.close();
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
            }
        }));
        System.out.println(
                "Java Provider demo listening on " + arguments.host + ":" + server.getPort());
        server.blockUntilShutdown();
    }

    private static final class EchoProvider extends ProviderService {
        @Override
        public String implementation() {
            return "java";
        }

        @Override
        public String invoke(String message) {
            return "java: " + message;
        }
    }

    private static final class Arguments {
        private final String host;
        private final int port;

        private Arguments(String host, int port) {
            this.host = host;
            this.port = port;
        }

        private static Arguments parse(String[] args) {
            String host = "127.0.0.1";
            int port = 50052;
            for (int index = 0; index < args.length; index++) {
                String option = args[index];
                if ("--host".equals(option)) {
                    host = requireValue(args, ++index, option);
                } else if ("--port".equals(option)) {
                    port = Integer.parseInt(requireValue(args, ++index, option));
                } else {
                    throw new IllegalArgumentException("unknown argument: " + option);
                }
            }
            return new Arguments(host, port);
        }

        private static String requireValue(String[] args, int index, String option) {
            if (index >= args.length) {
                throw new IllegalArgumentException("missing value for " + option);
            }
            return args[index];
        }
    }
}
