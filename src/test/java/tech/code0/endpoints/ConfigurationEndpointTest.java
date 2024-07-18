package tech.code0.endpoints;

import io.grpc.ManagedChannelBuilder;
import io.grpc.StatusRuntimeException;
import io.micronaut.context.ApplicationContext;
import io.micronaut.context.annotation.Property;
import io.micronaut.test.extensions.junit5.annotation.MicronautTest;
import jakarta.inject.Inject;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import tech.code0.model.ConfigurationOuterClass;
import tech.code0.model.ConfigurationServiceGrpc;
import tech.code0.model.FlowOuterClass;
import tech.code0.service.FlowService;

import static org.junit.jupiter.api.Assertions.*;

@MicronautTest
@Property(name = "update_based", value = "true")
public final class ConfigurationEndpointTest {

    @Inject
    FlowService flowService;  // Mock this service as needed

    @Inject
    ApplicationContext applicationContext;

    private ConfigurationServiceGrpc.ConfigurationServiceBlockingStub blockingStub;

    @BeforeEach
    void init() {
        final var channel = ManagedChannelBuilder.forAddress("localhost", 8080)
                .usePlaintext()
                .build();

        blockingStub = ConfigurationServiceGrpc.newBlockingStub(channel);
    }

    @Test
    void testUpdate() {
        final var flow = FlowOuterClass.Flow.newBuilder().setFlowId(1).build();
        final var configuration = ConfigurationOuterClass.Configuration.newBuilder()
                .setConfigurationId(1)
                .addFlows(flow)
                .build();

        final var request = ConfigurationOuterClass.UpdateConfigurationRequest.newBuilder()
                .setConfiguration(configuration)
                .build();

        final var response = blockingStub.update(request);

        assertNotNull(response);
        assertTrue(response.getSuccess());
    }

    @Test
    void testDelete() {
        final var flow = FlowOuterClass.Flow.newBuilder().setFlowId(1).build();
        final var configuration = ConfigurationOuterClass.Configuration.newBuilder()
                .setConfigurationId(1)
                .addFlows(flow)
                .build();

        final var request = ConfigurationOuterClass.DeleteConfigurationRequest.newBuilder()
                .setConfiguration(configuration)
                .build();

        final var response = blockingStub.delete(request);

        assertNotNull(response);
        assertTrue(response.getSuccess());
    }

    @Test
    void testUpdateWithEmptyFlows() {
        var configuration = ConfigurationOuterClass.Configuration.newBuilder()
                .setConfigurationId(1)
                .build();
        var request = ConfigurationOuterClass.UpdateConfigurationRequest.newBuilder()
                .setConfiguration(configuration)
                .build();

        assertThrows(StatusRuntimeException.class, () -> blockingStub.update(request));
    }


    @Test
    void testDeleteWithEmptyFlows() {
        var configuration = ConfigurationOuterClass.Configuration.newBuilder()
                .setConfigurationId(1)
                .build();
        var request = ConfigurationOuterClass.DeleteConfigurationRequest.newBuilder()
                .setConfiguration(configuration)
                .build();

        assertThrows(StatusRuntimeException.class, () -> blockingStub.delete(request));
    }
}
