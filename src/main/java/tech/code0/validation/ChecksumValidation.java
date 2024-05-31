package tech.code0.validation;

import lombok.experimental.UtilityClass;
import tech.code0.grpc.FlowOuterClass;

import java.time.Instant;
import java.util.Date;

@UtilityClass
public class ChecksumValidation {

    public boolean hasEqualTime(FlowOuterClass.Flow usedFlow, FlowOuterClass.Flow accurateFlow) {
        final var usedTime = Date.from(Instant.ofEpochSecond(usedFlow.getLastUpdated()));
        final var accurateTime = Date.from(Instant.ofEpochSecond(accurateFlow.getLastUpdated()));
        return usedTime.equals(accurateTime);
    }

}
