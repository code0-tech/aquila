package tech.code0.service;

import io.lettuce.core.api.StatefulRedisConnection;
import io.lettuce.core.api.async.RedisAsyncCommands;
import io.micronaut.runtime.context.scope.ThreadLocal;
import io.micronaut.scheduling.annotation.Scheduled;
import jakarta.inject.Singleton;
import tech.code0.model.FlowOuterClass;

import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.atomic.AtomicBoolean;

@Singleton
@ThreadLocal
public class FlowService {

    private final RedisAsyncCommands<String, String> commands;

    public FlowService(StatefulRedisConnection<String, String> connection) {
        this.commands = connection.async();
    }

    /**
     * Function to update a specific flow.
     *
     * @param organisationId of organisation that contains the flow.
     * @param flow           that should be updated.
     * @return <Boolean>true</Boolean> if update was successful.
     */
    public CompletableFuture<Boolean> updateFlow(long organisationId, FlowOuterClass.Flow flow) {
        final var response = this.commands.set(organisationId + ":" + flow.getFlowId(), flow.toString());
        return response.thenApply(string -> string.equals("OK")).toCompletableFuture();
    }

    /**
     * Function to update a list of flows.
     *
     * @param organisationId of organisation that contains the flows.
     * @param flows          that should be updated
     * @return <Boolean>true</Boolean> if update was successful.
     */
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

    /**
     * Function to delete a specific flow.
     *
     * @param organisationId of organisation that contains the flow.
     * @param flowId         of the flow that will be deleted.
     * @return <Boolean>true</Boolean> if update was successful.
     */
    public CompletableFuture<Boolean> deleteFlow(long organisationId, long flowId) {
        final var response = this.commands.del(organisationId + ":" + flowId);
        return response.thenApply(deletedFlows -> deletedFlows == 1).toCompletableFuture();
    }

    /**
     * Function to delete a list of flow.
     *
     * @param organisationId of organisation that contains the list of flows.
     * @param flows          that will be deleted.
     * @return <Boolean>true</Boolean> if update was successful.
     */
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