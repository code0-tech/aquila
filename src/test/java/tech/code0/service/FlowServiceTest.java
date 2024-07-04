package tech.code0.service;

import io.lettuce.core.api.StatefulRedisConnection;
import io.lettuce.core.api.async.RedisAsyncCommands;
import io.micronaut.test.extensions.junit5.annotation.MicronautTest;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.mockito.InjectMocks;
import org.mockito.Mock;
import org.mockito.MockitoAnnotations;
import tech.code0.model.FlowOuterClass;

import java.util.concurrent.CompletableFuture;

import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.*;

@MicronautTest
public class FlowServiceTest {

    @Mock
    private StatefulRedisConnection<String, String> connection;

    @Mock
    private RedisAsyncCommands<String, String> commands;

    @InjectMocks
    private FlowService flowService;

    @BeforeEach
    void setUp() {
        MockitoAnnotations.openMocks(this);
        when(connection.async()).thenReturn(commands);
    }

    @Test
    void testUpdateFlow() throws Exception {
        FlowOuterClass.Flow flow = FlowOuterClass.Flow.newBuilder().setFlowId(1).build();
        when(commands.set(eq("1:1"), any()));

        CompletableFuture<Boolean> result = flowService.updateFlow(1L, flow);

        assertTrue(result.get());
        verify(commands, times(1)).set(eq("1:1"), any());
    }
}
