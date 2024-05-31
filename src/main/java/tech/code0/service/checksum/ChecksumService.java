package tech.code0.service.checksum;

import com.google.common.util.concurrent.Futures;
import io.grpc.ManagedChannelBuilder;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import tech.code0.configuration.AquilaConfiguration;
import tech.code0.data.RedisConnection;
import tech.code0.data.flow.FlowService;
import tech.code0.grpc.FlowOuterClass;
import tech.code0.grpc.FlowServiceGrpc;

public class ChecksumService {

    private final Logger logger;
    private final RedisConnection connection;
    private final AquilaConfiguration configuration;
    private final FlowService flowService;

    public ChecksumService(RedisConnection connection, AquilaConfiguration configuration) {
        this.logger = LoggerFactory.getLogger(ChecksumService.class);
        this.connection = connection;
        this.configuration = configuration;

        this.flowService = new FlowService(connection.getAsyncCommands());
    }

    public void run(String configurationId) {
        final var managedChannel = ManagedChannelBuilder
                .forAddress(configuration.getBackendHost(), configuration.getBackendPort())
                .usePlaintext()
                .build();

        final var asyncStub = FlowServiceGrpc.newFutureStub(managedChannel);
        final var request = FlowOuterClass.FlowRequest.newBuilder()
                .setConfigurationId(configurationId)
                .build();

        final var response = asyncStub.getFlow(request);
        Futures.addCallback(response, new FlowCallback(flowService, connection, logger), Runnable::run);

    }
}