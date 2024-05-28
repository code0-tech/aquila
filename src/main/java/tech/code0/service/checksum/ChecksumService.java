package tech.code0.service.checksum;

import com.google.common.util.concurrent.Futures;
import io.grpc.ManagedChannelBuilder;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import tech.code0.configuration.AquilaConfiguration;
import tech.code0.data.RedisConnection;
import tech.code0.grpc.FlowOuterClass;
import tech.code0.grpc.FlowServiceGrpc;

public class ChecksumService {

    private final Logger logger = LoggerFactory.getLogger(ChecksumService.class);

    private final RedisConnection connection;
    private final AquilaConfiguration configuration;

    public ChecksumService(RedisConnection connection, AquilaConfiguration configuration) {
        this.connection = connection;
        this.configuration = configuration;
    }

    public void run(String flowId) {
        final var managedChannel = ManagedChannelBuilder
                .forAddress(configuration.getBackendHost(), configuration.getBackendPort())
                .usePlaintext()
                .build();

        final var asyncStub = FlowServiceGrpc.newFutureStub(managedChannel);
        final var request = FlowOuterClass.FlowRequest.newBuilder().setFlowId(flowId).build();
        final var response = asyncStub.getFlow(request);

        Futures.addCallback(response, new FlowCallback(), Runnable::run);
    }
}