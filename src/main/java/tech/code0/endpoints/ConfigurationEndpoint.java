package tech.code0.endpoints;

import io.grpc.stub.StreamObserver;
import io.micronaut.context.annotation.Requires;
import jakarta.inject.Inject;
import jakarta.inject.Singleton;
import tech.code0.model.ConfigurationOuterClass;
import tech.code0.model.ConfigurationServiceGrpc;
import tech.code0.service.FlowService;

@Singleton
@Requires(property = "update_based", value = "true")
public class ConfigurationEndpoint extends ConfigurationServiceGrpc.ConfigurationServiceImplBase {

    @Inject
    FlowService flowService;

    /**
     * Endpoint for updating all received flows in redis
     *
     * @param request          to update flows, contains new/updated flows and information about the current configuration.
     * @param responseObserver to observe and answer a request.
     */
    @Override
    public void update(ConfigurationOuterClass.UpdateConfigurationRequest request, StreamObserver<ConfigurationOuterClass.UpdateConfigurationResponse> responseObserver) {
        final var configuration = request.getConfiguration();
        if (configuration.getFlowsCount() == 0) return;

        final var response = this.flowService.updateFlows(configuration.getConfigurationId(), configuration.getFlowsList());
        response.thenAccept(success -> {
            final var answer = ConfigurationOuterClass.UpdateConfigurationResponse.newBuilder().setSuccess(success).build();

            responseObserver.onNext(answer);
            responseObserver.onCompleted();
        });
    }

    /**
     * Endpoint for deleting all received flows in redis
     *
     * @param request          to delete flows, contains flows and information about the current configuration.
     * @param responseObserver to observe and answer a request.
     */
    @Override
    public void delete(ConfigurationOuterClass.DeleteConfigurationRequest request, StreamObserver<ConfigurationOuterClass.DeleteConfigurationResponse> responseObserver) {
        final var configuration = request.getConfiguration();
        if (configuration.getFlowsCount() == 0) return;

        final var response = this.flowService.deleteFlows(configuration.getConfigurationId(), configuration.getFlowsList());
        response.thenAccept(success -> {
            final var answer = ConfigurationOuterClass.DeleteConfigurationResponse.newBuilder().setSuccess(success).build();

            responseObserver.onNext(answer);
            responseObserver.onCompleted();
        });
    }
}