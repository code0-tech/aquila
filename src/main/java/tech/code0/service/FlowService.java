package tech.code0.service;

import io.lettuce.core.api.StatefulRedisConnection;
import io.lettuce.core.api.async.RedisAsyncCommands;
import io.micronaut.runtime.context.scope.ThreadLocal;
import io.micronaut.scheduling.annotation.Scheduled;
import tech.code0.model.FlowOuterClass;

import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.atomic.AtomicBoolean;

@ThreadLocal
public class FlowService {

    private final RedisAsyncCommands<String, String> commands;

    public FlowService(StatefulRedisConnection<String, String> connection) {
        this.commands = connection.async();
    }

    public CompletableFuture<Boolean> updateFlow(long organisationId, FlowOuterClass.Flow flow) {
        final var response = this.commands.set(organisationId + ":" + flow.getFlowId(), flow.toString());
        return response.thenApply(string -> string.equals("OK")).toCompletableFuture();
    }

    public CompletableFuture<Boolean> updateFlows(long organisationId, List<FlowOuterClass.Flow> flows) {
        AtomicBoolean success = new AtomicBoolean(true);

        for (final var flow : flows) {
            final var response = this.commands.set(organisationId + ":" + flow.getFlowId(), flow.toString());
            response.thenAccept(string -> {
                if (!string.equals("OK")) success.set(false);
            });
        }

        return CompletableFuture.completedFuture(success.get());
    }

    public CompletableFuture<Boolean> deleteFlow(long flowId, long organisationId) {
        final var response = this.commands.del(organisationId + ":" + flowId);
        return response.thenApply(deletedFlows -> deletedFlows == 1).toCompletableFuture();
    }

    public CompletableFuture<Boolean> deleteFlows(long organisationId, List<FlowOuterClass.Flow> flows) {
        final var keys = flows.stream().map(flow -> organisationId + ":" + flow.getFlowId()).toList();
        final var response = this.commands.del(keys.toArray(new String[0]));
        return response.thenApply(deletedFlows -> deletedFlows == flows.size()).toCompletableFuture();
    }

    @Scheduled()
    private void overWrite(long organisationId, List<FlowOuterClass.Flow> flows) {
        final var response = this.commands.keys(organisationId + ":");
        response.thenAccept(keys -> this.commands.del(keys.toArray(new String[0])));
        this.updateFlows(organisationId, flows);
    }

}