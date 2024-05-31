package tech.code0.service.checksum;

import com.google.common.util.concurrent.FutureCallback;
import io.grpc.Status;
import org.jetbrains.annotations.NotNull;
import org.slf4j.Logger;
import tech.code0.data.RedisConnection;
import tech.code0.data.flow.FlowService;
import tech.code0.grpc.FlowOuterClass;
import tech.code0.validation.ChecksumValidation;

public class FlowCallback implements FutureCallback<FlowOuterClass.FlowResponse> {

    private final Logger logger;
    private final RedisConnection connection;
    private final FlowService flowService;

    public FlowCallback(FlowService flowService, RedisConnection redisConnection, Logger logger) {
        this.logger = logger;
        this.flowService = flowService;
        this.connection = redisConnection;
    }

    @Override
    public void onSuccess(FlowOuterClass.FlowResponse result) {
        result.getFlowsList().forEach(this::checkFlow);
    }

    @Override
    public void onFailure(@NotNull Throwable throwable) {
        final var status = Status.fromThrowable(throwable.getCause());
        this.logger.error(STR."Flow couldn't be recived\{status.getDescription()}", throwable);
    }

    private void checkFlow(FlowOuterClass.Flow reponseFlow) {
        flowService.getFlow(reponseFlow.getFlowId()).thenAccept(currentFlow -> {
            final var isOutdated = ChecksumValidation.hasEqualTime(currentFlow, reponseFlow);
            if (!isOutdated) return;
            this.connection.getConnection().async().set(STR."flow:\{currentFlow.getFlowId()}", reponseFlow.toString());
        });
    }
}