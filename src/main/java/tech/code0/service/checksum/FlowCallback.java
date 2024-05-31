package tech.code0.service.checksum;

import com.google.common.util.concurrent.FutureCallback;
import io.grpc.Status;
import org.jetbrains.annotations.NotNull;
import org.slf4j.Logger;
import tech.code0.data.RedisConnection;
import tech.code0.grpc.FlowOuterClass;
import tech.code0.validation.ChecksumValidation;

public class FlowCallback implements FutureCallback<FlowOuterClass.FlowResponse> {

    private final Logger logger;
    private final RedisConnection connection;

    public FlowCallback(RedisConnection redisConnection, Logger logger) {
        this.logger = logger;
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
        connection.getFlow(reponseFlow.getFlowId()).thenAccept(optionalFlow -> {

            if (optionalFlow.isEmpty()) {
                this.connection.getConnection().async().set(STR."flow:\{reponseFlow.getFlowId()}", reponseFlow.toString());
                this.logger.info(STR."Flow with \{reponseFlow.getFlowId()} wasn't present in redis. Inserted response!");
                return;
            }

            final var currentFlow = optionalFlow.get();
            final var isOutdated = ChecksumValidation.hasEqualTime(currentFlow, reponseFlow);
            if (!isOutdated) return;

            this.connection.getConnection().async().set(STR."flow:\{currentFlow.getFlowId()}", reponseFlow.toString());
            this.logger.info(STR."Flow with id: \{currentFlow.getFlowId()} was outdated and was overwritten!");
        });
    }
}